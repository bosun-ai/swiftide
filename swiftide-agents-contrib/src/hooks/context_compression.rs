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
use swiftide_agents::{Agent, AgentBuilder, hooks::AfterEachFn};
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
    doc = "Configuration for `ContextCompression`.\n\n# Examples\n\n```no_run\nuse std::sync::Arc;\nuse swiftide_agents::AgentBuilder;\nuse swiftide_agents_contrib::hooks::ContextCompression;\nuse swiftide_core::ChatCompletion;\nfn attach(agent: &mut AgentBuilder, model: Arc<dyn ChatCompletion>) -> anyhow::Result<()> {\n    let hook = ContextCompression::builder().llm(model).git_start_sha(\"HEAD\")\n        .git_diff_dir(\".\").build()?;\n    hook.register_hooks(agent);\n    Ok(())\n}\n```"
)]
#[builder(setter(into))]
/// Summarize conversation history when the turn or context threshold is reached.
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
///     hook.register_hooks(agent);
///     Ok(())
/// }
/// ```
pub struct ContextCompression {
    /// Set the model used to summarize conversation history.
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
    ///     hook.register_hooks(agent);
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
    async fn should_summarize(self: &Arc<Self>, agent: &Agent, reply: Option<&str>) -> Reason {
        let current_count = self
            .num_completions_since_summary
            .load(std::sync::atomic::Ordering::SeqCst);

        // Get the last message before filtering, as filtering will merge tool outputs with
        // assistant messages to avoid tool call/output mismatches
        let mut history = agent.context().history().await.ok().unwrap_or_default();
        let maybe_last = history.last().cloned();
        if let Some(reply) = reply {
            history.push(ChatMessage::new_assistant(Some(reply), None));
        }
        let history = self.request_history(history);

        let estimate = history
            .iter()
            .map(|m| m.as_ref().chars().count())
            .sum::<usize>()
            / 4;

        if let Some(ChatMessage::ToolOutput(tool_call, _)) = maybe_last
            && reply.is_none()
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

    /// Compress text replies before they are recorded, and tool turns after their outputs.
    ///
    /// A final assistant reply stays last in history, so compression cannot restart a finished
    /// conversation. Hooks keep no pending response or request payload shared between agents.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use swiftide_agents_contrib::hooks::ContextCompression;
    /// # fn configure(compression: ContextCompression, builder: &mut swiftide_agents::AgentBuilder) {
    /// compression.register_hooks(builder);
    /// # }
    /// ```
    pub fn register_hooks(self, builder: &mut AgentBuilder) {
        let text = Arc::new(self.clone());
        builder.after_completion(move |agent, response| {
            let text = Arc::clone(&text);
            let tool_free = response.tool_calls.as_ref().is_none_or(Vec::is_empty);
            let reply = response.message.clone();
            Box::pin(async move {
                if tool_free && agent.is_running() {
                    let empty = reply.is_none();
                    if text.summarize_if_needed(agent, false, reply).await? && empty {
                        // An empty completion must not turn the summary into another request.
                        agent.context().next_completion().await?;
                    }
                }
                Ok(())
            })
        });
        let tools = Arc::new(self);
        builder.after_each(move |agent| {
            let tools = Arc::clone(&tools);
            Box::pin(async move {
                if !agent.is_running() {
                    return Ok(());
                }
                let history = agent.context().history().await?;
                if matches!(history.last(), Some(ChatMessage::ToolOutput(..))) {
                    let input = agent.context().current_new_messages().await?;
                    // An empty reply leaves the input's last tool output unchanged. Only count a
                    // tool turn when it appended a new output (tool calls carry distinct IDs).
                    if history.last() != input.last() {
                        tools.summarize_if_needed(agent, true, None).await?;
                    }
                }
                Ok(())
            })
        });
    }

    #[must_use = "the hook has no effect until it is registered on an agent"]
    /// Summarize completed turns and add a continuation message for an active conversation.
    ///
    /// Prefer [`Self::register_hooks`] to also compress final text replies without restarting the
    /// agent. This hook is intended for callers that already manage the agent's stopping behavior.
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
                    this.summarize_if_needed(agent, true, None)
                        .await
                        .map(|_| ())
                }
                .instrument(span),
            )
        }
    }

    async fn summarize_if_needed(
        self: &Arc<Self>,
        agent: &mut Agent,
        continue_after_summary: bool,
        reply: Option<String>,
    ) -> Result<bool> {
        let reason = self.should_summarize(agent, reply.as_deref()).await;

        if reason.should_summarize() {
            tracing::info!(
                ?reason,
                "[ContextCompression] Conversation summarizer triggered"
            );

            self.summarize_conversation(agent, continue_after_summary, reply)
                .await
        } else {
            self.num_completions_since_summary
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            tracing::debug!("[ContextCompression] Conversation summarizer not triggered");
            Ok(false)
        }
    }

    /// The history since the last summary, as the model is sent it.
    fn request_history(&self, mut history: Vec<ChatMessage>) -> Vec<ChatMessage> {
        if let Some(prepare) = self.prepare_history {
            prepare(&mut history);
        }
        filter_messages_since_summary(history)
    }

    async fn summarize_conversation(
        self: &Arc<Self>,
        agent: &mut Agent,
        continue_after_summary: bool,
        reply: Option<String>,
    ) -> Result<bool> {
        // Reset the counter
        self.num_completions_since_summary
            .store(0, std::sync::atomic::Ordering::SeqCst);

        let prompt = render_prompt(&self.git_start_sha, agent, &self.git_diff_dir).await?;
        let mut messages = agent.context().history().await?;
        if let Some(reply) = reply {
            messages.push(ChatMessage::new_assistant(Some(reply), None));
        }
        let mut messages = self.request_history(messages);
        messages.push(ChatMessage::new_user(prompt));

        let summary = self.llm.complete(&messages.into()).await?;

        if let Some(summary) = summary.message() {
            tracing::debug!(summary = %summary, "Summarized conversation");
            agent.add_message(ChatMessage::new_summary(summary)).await?;
            // Active tool turns need a user message after assistant summaries for Mistral.
            // Finished queries wait for the next real user message instead.
            if continue_after_summary {
                agent
                    .context()
                    .add_message(ChatMessage::new_user(CONTINUE_AFTER_SUMMARY))
                    .await?;
            }
            install_original_instructions_recovery(agent).await?;
            Ok(true)
        } else {
            tracing::error!("No summary generated, this is a bug");
            Ok(false)
        }
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
    /// Configure conversation compression.
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
    ///     hook.register_hooks(agent);
    ///     Ok(())
    /// }
    /// ```
    pub fn builder() -> ContextCompressionBuilder {
        ContextCompressionBuilder::default()
    }
}

impl ContextCompressionBuilder {
    /// Build the conversation compression hook.
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
    ///     hook.register_hooks(agent);
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
