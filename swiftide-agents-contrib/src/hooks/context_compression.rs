//! This module implements a conversation summarizer
//!
//! The summarizer counts number of completions in an agent, and if the count surpasses a set
//! value, the conversation is summarized and a summary is added to the conversation.
//!
//! Often when there are many messages, the context window can get too large for the agent to
//! effectively complete. This summarizer helps to keep the context window small and steers the
//! agent towards a more focused solution.
//!
//! Because it also acts as a nice opportunity to steer, we will also include a steering prompt
//! and the current diff
//!
//! The agent completes messages since the last summary.
mod original_instructions;
pub(super) use original_instructions::install as install_original_instructions_recovery;

use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicUsize},
};

use anyhow::Result;
use derive_builder::Builder;
use regex::Regex;
use serde::Serialize;
use swiftide_agents::{Agent, hooks::AfterEachFn};
use swiftide_core::{
    ChatCompletion, Command,
    chat_completion::{ChatMessage, Tool},
};
use tracing::Instrument as _;

use crate::support::{Templates, accept_non_zero_exit, is_file_read_tool};

const TEMPLATE_NAME: &str = "conversation_summary.md";
const CONTINUE_AFTER_SUMMARY: &str = "Continue";

#[derive(Debug, Clone, Serialize)]
struct PlanEntry {
    status: String,
    step: String,
}

#[derive(Clone, Builder)]
#[builder(build_fn(name = "build_inner", private))]
#[builder_struct_attr(
    doc = "Configuration for `ContextCompression`.\n\n# Examples\n\n```no_run\nuse std::sync::Arc;\nuse swiftide_agents::AgentBuilder;\nuse swiftide_agents_contrib::hooks::ContextCompression;\nuse swiftide_core::ChatCompletion;\nfn attach(agent: &mut AgentBuilder, model: Arc<dyn ChatCompletion>) -> anyhow::Result<()> {\n    let hook = ContextCompression::builder().llm(model).git_start_sha(\"HEAD\")\n        .git_diff_dir(\".\").build()?;\n    agent.after_each(hook.summarize_hook());\n    Ok(())\n}\n```"
)]
#[builder(setter(into))]
/// The `ContextCompression` hook from Fluyt, with its original defaults and control flow.
///
/// # Examples
///
/// ```no_run
/// use std::sync::Arc;
/// use swiftide_agents::AgentBuilder;
/// use swiftide_agents_contrib::hooks::ContextCompression;
/// use swiftide_core::ChatCompletion;
/// fn attach(agent: &mut AgentBuilder, model: Arc<dyn ChatCompletion>) -> anyhow::Result<()> {
///     let hook = ContextCompression::builder().llm(model).git_start_sha("HEAD")
///         .git_diff_dir(".").build()?;
///     agent.after_each(hook.summarize_hook());
///     Ok(())
/// }
/// ```
pub struct ContextCompression {
    /// Configure `llm` using the same setting as Fluyt.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use swiftide_agents::AgentBuilder;
    /// use swiftide_agents_contrib::hooks::ContextCompression;
    /// use swiftide_core::ChatCompletion;
    /// fn attach(agent: &mut AgentBuilder, model: Arc<dyn ChatCompletion>) -> anyhow::Result<()> {
    ///     let hook = ContextCompression::builder().llm(model).git_start_sha("HEAD")
    ///         .git_diff_dir(".").build()?;
    ///     agent.after_each(hook.summarize_hook());
    ///     Ok(())
    /// }
    /// ```
    llm: Arc<dyn ChatCompletion>,
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents_contrib::hooks::ContextCompression;
    /// let mut builder = ContextCompression::builder();
    /// builder.git_start_sha("HEAD");
    /// ```
    git_start_sha: String,
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents_contrib::hooks::ContextCompression;
    /// let mut builder = ContextCompression::builder();
    /// builder.git_diff_dir(".");
    /// ```
    git_diff_dir: PathBuf,

    #[builder(private, default = Arc::new(AtomicUsize::new(0)))]
    num_completions_since_summary: Arc<AtomicUsize>,
    #[builder(default = 5)]
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents_contrib::hooks::ContextCompression;
    /// let mut builder = ContextCompression::builder();
    /// builder.num_completions_for_summary(5usize);
    /// ```
    num_completions_for_summary: usize,
    #[builder(default = 400_000_000)]
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents_contrib::hooks::ContextCompression;
    /// let mut builder = ContextCompression::builder();
    /// builder.context_limit(32_000usize);
    /// ```
    context_limit: usize,
    // #[builder(default = TikToken::default())]
    // token_estimator: TikToken,
    #[builder(default = 0.7)]
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents_contrib::hooks::ContextCompression;
    /// let mut builder = ContextCompression::builder();
    /// builder.context_cutoff_perc(0.7);
    /// ```
    context_cutoff_perc: f64,
    #[builder(default)]
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents_contrib::hooks::ContextCompression;
    /// let mut builder = ContextCompression::builder();
    /// builder.prepare_history(None::<fn(&mut [swiftide_core::chat_completion::ChatMessage])>);
    /// ```
    prepare_history: Option<fn(&mut [ChatMessage])>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Reason {
    // Summarize because turns exceeded the threshold, includes the number of turns
    TurnsExceeded,

    // Current conversation is too long, and we need to summarize
    TokenLimitExceeded,

    // Current conversation is too long, need to summarize. Probably requires another file read
    TokenLimitExceededFileRead,

    // These are OK
    SkipFileRead,
    SkipAgentStopped,
    Skip,
}

impl Reason {
    fn should_summarize(&self) -> bool {
        match self {
            Reason::SkipFileRead | Reason::SkipAgentStopped | Reason::Skip => false,
            Reason::TurnsExceeded
            | Reason::TokenLimitExceeded
            | Reason::TokenLimitExceededFileRead => true,
        }
    }
}

impl ContextCompression {
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_precision_loss)]
    #[allow(clippy::cast_possible_truncation)]
    async fn should_summarize(self: &Arc<Self>, agent: &Agent) -> Reason {
        let current_count = self
            .num_completions_since_summary
            .load(std::sync::atomic::Ordering::SeqCst);

        // Get the last message before filtering, as filtering will merge tool outputs with
        // assistant messages to avoid tool call/output mismatches
        let history = agent.context().history().await.ok().unwrap_or_default();
        let maybe_last = history.last().cloned();
        let history = self.request_history(history);

        let estimate = history
            .iter()
            .map(|m| m.as_ref().chars().count())
            .sum::<usize>()
            / 4;

        if let Some(ChatMessage::ToolOutput(tool_call, _)) = maybe_last
            && is_file_read_tool(tool_call.name())
        {
            if estimate < (self.context_limit as f64 * 0.9) as usize {
                tracing::debug!(
                    estimate,
                    self.context_limit,
                    current_count,
                    "Last message output from a file-read tool, postponing summary, current_count: {current_count}",
                );
                return Reason::SkipFileRead;
            }
            tracing::debug!(
                estimate,
                self.context_limit,
                current_count,
                "Last message output from a file-read tool, too close to context limit, summarizing",
            );

            return Reason::TokenLimitExceededFileRead;
        }

        // If the number of completions since the last summary is greater than or equal to the
        if self.num_completions_for_summary > 0 && current_count >= self.num_completions_for_summary
        {
            return Reason::TurnsExceeded;
        }

        // If the Agent stopped, we don't summarize
        if agent.is_stopped() {
            tracing::debug!(
                "Agent is stopped, not summarizing conversation, current count: {}",
                current_count
            );
            return Reason::SkipAgentStopped;
        }

        if estimate > (self.context_limit as f64 * self.context_cutoff_perc) as usize {
            return Reason::TokenLimitExceeded;
        }
        tracing::trace!(
            "Conversation is within limits, current count: {}, estimate: {}",
            current_count,
            estimate
        );

        Reason::Skip
    }

    #[must_use = "the hook has no effect until it is registered on an agent"]
    /// Register the original after-each hook.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use swiftide_agents::AgentBuilder;
    /// use swiftide_agents_contrib::hooks::ContextCompression;
    /// use swiftide_core::ChatCompletion;
    /// fn attach(agent: &mut AgentBuilder, model: Arc<dyn ChatCompletion>) -> anyhow::Result<()> {
    ///     let hook = ContextCompression::builder().llm(model).git_start_sha("HEAD")
    ///         .git_diff_dir(".").build()?;
    ///     agent.after_each(hook.summarize_hook());
    ///     Ok(())
    /// }
    /// ```
    pub fn summarize_hook(self) -> impl AfterEachFn {
        let this = Arc::new(self);
        move |agent: &mut Agent| {
            let this = this.clone();
            let span = tracing::info_span!("summarize_conversation");
            Box::pin(
                async move {
                    let reason = this.should_summarize(agent).await;

                    if reason.should_summarize() {
                        tracing::info!(
                            ?reason,
                            "[ContextCompression] Conversation summarizer triggered"
                        );

                        this.summarize_conversation(agent).await
                    } else {
                        this.num_completions_since_summary
                            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        tracing::debug!(
                            "[ContextCompression] Conversation summarizer not triggered"
                        );
                        Ok(())
                    }
                }
                .instrument(span),
            )
        }
    }

    /// The history since the last summary, as the model is sent it.
    fn request_history(&self, mut history: Vec<ChatMessage>) -> Vec<ChatMessage> {
        if let Some(prepare) = self.prepare_history {
            prepare(&mut history);
        }
        filter_messages_since_summary(history)
    }

    async fn summarize_conversation(self: &Arc<Self>, agent: &mut Agent) -> Result<()> {
        // Reset the counter
        self.num_completions_since_summary
            .store(0, std::sync::atomic::Ordering::SeqCst);

        let prompt = render_prompt(&self.git_start_sha, agent, &self.git_diff_dir).await?;
        let mut messages = self.request_history(agent.context().history().await?);
        messages.push(ChatMessage::new_user(prompt));

        let summary = self.llm.complete(&messages.into()).await?;

        if let Some(summary) = summary.message() {
            tracing::debug!(summary = %summary, "Summarized conversation");
            agent.add_message(ChatMessage::new_summary(summary)).await?;
            // Summaries are sent as assistant messages by the OpenAI-compatible client. Mistral
            // requires the final message to be from a user or tool, so add a hidden continuation
            // turn directly to the context after compression.
            agent
                .context()
                .add_message(ChatMessage::new_user(CONTINUE_AFTER_SUMMARY))
                .await?;
            install_original_instructions_recovery(agent).await?;
        } else {
            tracing::error!("No summary generated, this is a bug");
        }

        Ok(())
    }
}

fn extract_plan_from_history(history: &[ChatMessage]) -> Option<Vec<PlanEntry>> {
    let regex = Regex::new(r"(?m)^\d+\.\s+\[(?P<status>[^\]]+)\]\s+(?P<step>.+?)\s*$")
        .expect("valid regex");

    history.iter().rev().find_map(|message| {
        if let ChatMessage::ToolOutput(tool_call, tool_output) = message {
            if tool_call.name() != "update_plan" {
                return None;
            }

            let content = tool_output.content()?;
            let (_, content) = content.split_once("# Current Plan\n")?;
            let content = content.split("\n\n").next()?;
            let plan_items: Vec<PlanEntry> = regex
                .captures_iter(content)
                .map(|caps| PlanEntry {
                    status: caps["status"].trim().to_string(),
                    step: caps["step"].trim().to_string(),
                })
                .filter(|entry| !entry.step.is_empty())
                .collect();

            if plan_items.is_empty() {
                None
            } else {
                Some(plan_items)
            }
        } else {
            None
        }
    })
}

async fn render_prompt(git_start_sha: &str, agent: &Agent, diff_path: &Path) -> Result<String> {
    let history = agent.context().history().await?;
    let initial_user_message = history.iter().find(|m| m.is_user()).and_then(|m| match m {
        ChatMessage::User(message) => Some(message),
        _ => None,
    });
    let plan = extract_plan_from_history(&history);
    let available_tools = agent
        .tools()
        .iter()
        .map(|tool| {
            format!(
                "- **{}**: {}",
                tool.name(),
                // Some tools have large descriptions, only take the first line
                tool.tool_spec()
                    .description
                    .lines()
                    .take(1)
                    .collect::<String>()
            )
        })
        .collect::<Vec<String>>()
        .join("\n");

    let diff = accept_non_zero_exit(
        agent
            .context()
            .executor()
            .exec_cmd(
                &Command::shell(format!("git diff {git_start_sha} --no-color"))
                    .with_current_dir(diff_path),
            )
            .await,
    )?
    .into_string_lossy();

    let mut context = tera::Context::from_serialize(serde_json::json!({
        "initial_user_message": initial_user_message,
        "available_tools": available_tools,
        "diff": diff,
    }))?;

    if let Some(plan_items) = plan {
        context.insert("plan", &plan_items);
    }

    let rendered = Templates::render(TEMPLATE_NAME, &context)?;

    Ok(rendered)
}

fn filter_messages_since_summary<I: IntoIterator<Item = ChatMessage>>(
    messages: I,
) -> Vec<ChatMessage>
where
    <I as IntoIterator>::IntoIter: DoubleEndedIterator,
{
    // Filter out all messages up to and including the last summary
    let mut summary_found = false;
    let mut messages = messages
        .into_iter()
        .rev()
        .filter_map(|m| {
            // If we have found a summary, we are good
            if summary_found {
                return None;
            }

            // Ignore the system prompt, it only misdirects
            if matches!(m, ChatMessage::System(..)) {
                return None;
            }

            // Tool outputs are formatted as assistant messages
            if let ChatMessage::ToolOutput(tool_call,tool_output) = &m {
                let message = format!("I ran a tool called: {} with the following arguments: {}\n The tool returned:\n{}", tool_call.name(), tool_call.args().unwrap_or("No arguments"), tool_output.content().unwrap_or("No output"));
                return Some(ChatMessage::Assistant(Some(message), None));
            }

            // For assistant messages, we only keep those with messages in them
            if let ChatMessage::Assistant(message, Some(..)) = &m {
                if message.is_some() {
                    return Some(ChatMessage::Assistant(message.clone(), None));
                }
                return None;
            }
            if let ChatMessage::Summary(_) = m {
                summary_found = true;
            }
            Some(m.clone())
        })
        .collect::<Vec<_>>();

    messages.reverse();

    messages
}

impl ContextCompression {
    /// Configure the hook using Fluyt's defaults.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use swiftide_agents::AgentBuilder;
    /// use swiftide_agents_contrib::hooks::ContextCompression;
    /// use swiftide_core::ChatCompletion;
    /// fn attach(agent: &mut AgentBuilder, model: Arc<dyn ChatCompletion>) -> anyhow::Result<()> {
    ///     let hook = ContextCompression::builder().llm(model).git_start_sha("HEAD")
    ///         .git_diff_dir(".").build()?;
    ///     agent.after_each(hook.summarize_hook());
    ///     Ok(())
    /// }
    /// ```
    pub fn builder() -> ContextCompressionBuilder {
        ContextCompressionBuilder::default()
    }
}

impl ContextCompressionBuilder {
    /// Build the hook. Required settings match the source builder.
    ///
    /// # Errors
    ///
    /// Returns an error when a required setting is missing.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use swiftide_agents::AgentBuilder;
    /// use swiftide_agents_contrib::hooks::ContextCompression;
    /// use swiftide_core::ChatCompletion;
    /// fn attach(agent: &mut AgentBuilder, model: Arc<dyn ChatCompletion>) -> anyhow::Result<()> {
    ///     let hook = ContextCompression::builder().llm(model).git_start_sha("HEAD")
    ///         .git_diff_dir(".").build()?;
    ///     agent.after_each(hook.summarize_hook());
    ///     Ok(())
    /// }
    /// ```
    pub fn build(&self) -> Result<ContextCompression, ContextCompressionBuilderError> {
        self.build_inner()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plan_recovery_ignores_explanation_lines() {
        let call = swiftide_core::chat_completion::ToolCall::builder()
            .name("update_plan")
            .id("plan")
            .build()
            .unwrap();
        let history = [ChatMessage::new_tool_output(
            call,
            "# Current Plan\n1. [pending] Inspect\n\n2. [completed] Deploy",
        )];
        let plan = extract_plan_from_history(&history).unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].step, "Inspect");
    }
}
