use std::{collections::HashSet, sync::Arc};

use derive_builder::Builder;
use swiftide_agents::{Agent, hooks::AfterToolFn};
use swiftide_core::{
    Command, SimplePrompt,
    chat_completion::{Tool, ToolCall, ToolOutput},
    prompt::Prompt,
};
use tracing::Instrument as _;

use crate::support::{FILE_READ_TOOL_NAMES, Templates, accept_non_zero_exit};

const TOOLS_NEEDING_DIFF: &[&str] = &["run_tests", "run_coverage"];
const DEFAULT_SUMMARIZE_TOKENS_CUTOFF: usize = 20_000;
const DEFAULT_TRUNCATE_TOKEN_CUTOFF: usize = 10_000;
const DEFAULT_TRUNCATE_HEAD_LINES: usize = 40;
const DEFAULT_TRUNCATE_TAIL_LINES: usize = 160;
const TRUNCATION_PLACEHOLDER: &str = "<content trimmed>";

#[derive(Clone, Builder)]
#[builder(build_fn(name = "build_inner", private))]
#[builder_struct_attr(
    doc = "Configuration for `ToolSummarizer`.\n\n# Examples\n\n```no_run\nuse std::sync::Arc;\nuse swiftide_agents::AgentBuilder;\nuse swiftide_agents_contrib::hooks::ToolSummarizer;\nuse swiftide_core::SimplePrompt;\nfn attach(agent: &mut AgentBuilder, model: Arc<dyn SimplePrompt>) -> anyhow::Result<()> {\n    let hook = ToolSummarizer::builder().llm(model).tools_to_summarize(Vec::new())\n        .git_start_sha(\"HEAD\").build()?;\n    agent.after_tool(hook.summarize_hook());\n    Ok(())\n}\n```"
)]
#[builder(setter(strip_option))]
/// The `ToolSummarizer` hook from Fluyt, with its original defaults and control flow.
///
/// # Examples
///
/// ```no_run
/// use std::sync::Arc;
/// use swiftide_agents::AgentBuilder;
/// use swiftide_agents_contrib::hooks::ToolSummarizer;
/// use swiftide_core::SimplePrompt;
/// fn attach(agent: &mut AgentBuilder, model: Arc<dyn SimplePrompt>) -> anyhow::Result<()> {
///     let hook = ToolSummarizer::builder().llm(model).tools_to_summarize(Vec::new())
///         .git_start_sha("HEAD").build()?;
///     agent.after_tool(hook.summarize_hook());
///     Ok(())
/// }
/// ```
pub struct ToolSummarizer {
    /// The LLM that will be used to summarize the tool output.
    #[builder(setter(into))]
    /// Configure `llm` using the same setting as Fluyt.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use swiftide_agents::AgentBuilder;
    /// use swiftide_agents_contrib::hooks::ToolSummarizer;
    /// use swiftide_core::SimplePrompt;
    /// fn attach(agent: &mut AgentBuilder, model: Arc<dyn SimplePrompt>) -> anyhow::Result<()> {
    ///     let hook = ToolSummarizer::builder().llm(model).tools_to_summarize(Vec::new())
    ///         .git_start_sha("HEAD").build()?;
    ///     agent.after_tool(hook.summarize_hook());
    ///     Ok(())
    /// }
    /// ```
    llm: Arc<dyn SimplePrompt>,

    /// The tools that should always be summarized.
    #[builder(setter(into))]
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents_contrib::hooks::ToolSummarizer;
    /// let mut builder = ToolSummarizer::builder();
    /// builder.tools_to_summarize(vec!["run_tests"]);
    /// ```
    tools_to_summarize: Vec<&'static str>,

    /// The git SHA to use as the starting point for diffs.
    #[builder(setter(into))]
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents_contrib::hooks::ToolSummarizer;
    /// let mut builder = ToolSummarizer::builder();
    /// builder.git_start_sha("HEAD");
    /// ```
    git_start_sha: String,

    /// Enable automatic summarization of all tools that exceed a certain token count.
    #[builder(default = None)]
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents_contrib::hooks::ToolSummarizer;
    /// let mut builder = ToolSummarizer::builder();
    /// builder.summarize_all_at_token_cutoff(20_000usize);
    /// ```
    summarize_all_at_token_cutoff: Option<usize>,

    /// Tools when summarized that include the git diff
    #[builder(setter(into), default = Some(TOOLS_NEEDING_DIFF.iter().map(|s| *s).collect()))]
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents_contrib::hooks::ToolSummarizer;
    /// let mut builder = ToolSummarizer::builder();
    /// builder.tools_needing_diff(vec!["run_tests"]);
    /// ```
    tools_needing_diff: Option<Vec<&'static str>>,

    /// Tools to always exclude from automatic summarization.
    #[builder(setter(into), default = Some(FILE_READ_TOOL_NAMES.to_vec()))]
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents_contrib::hooks::ToolSummarizer;
    /// let mut builder = ToolSummarizer::builder();
    /// builder.skip_summarize_all(vec!["Read"]);
    /// ```
    skip_summarize_all: Option<Vec<&'static str>>,

    /// Truncate tool output before summarizing when estimated tokens exceed this cutoff.
    #[builder(setter(strip_option), default = Some(DEFAULT_TRUNCATE_TOKEN_CUTOFF))]
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents_contrib::hooks::ToolSummarizer;
    /// let mut builder = ToolSummarizer::builder();
    /// builder.truncate_tokens_cutoff(10_000usize);
    /// ```
    truncate_tokens_cutoff: Option<usize>,

    /// Number of leading lines to retain when truncating.
    #[builder(default = DEFAULT_TRUNCATE_HEAD_LINES)]
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents_contrib::hooks::ToolSummarizer;
    /// let mut builder = ToolSummarizer::builder();
    /// builder.truncate_head_lines(40usize);
    /// ```
    truncate_head_lines: usize,

    /// Number of trailing lines to retain when truncating. Tail is favoured over head.
    #[builder(default = DEFAULT_TRUNCATE_TAIL_LINES)]
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents_contrib::hooks::ToolSummarizer;
    /// let mut builder = ToolSummarizer::builder();
    /// builder.truncate_tail_lines(160usize);
    /// ```
    truncate_tail_lines: usize,
}

impl ToolSummarizer {
    #[must_use]
    /// Configure the hook using Fluyt's defaults.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use swiftide_agents::AgentBuilder;
    /// use swiftide_agents_contrib::hooks::ToolSummarizer;
    /// use swiftide_core::SimplePrompt;
    /// fn attach(agent: &mut AgentBuilder, model: Arc<dyn SimplePrompt>) -> anyhow::Result<()> {
    ///     let hook = ToolSummarizer::builder().llm(model).tools_to_summarize(Vec::new())
    ///         .git_start_sha("HEAD").build()?;
    ///     agent.after_tool(hook.summarize_hook());
    ///     Ok(())
    /// }
    /// ```
    pub fn builder() -> ToolSummarizerBuilder {
        ToolSummarizerBuilder::default()
    }

    // Rust has a long outstanding issue where it captures outer lifetimes when returning an impl
    // that also has lifetimes, see https://github.com/rust-lang/rust/issues/42940
    #[must_use = "the hook has no effect until it is registered on an agent"]
    /// Register the original after-tool hook.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use swiftide_agents::AgentBuilder;
    /// use swiftide_agents_contrib::hooks::ToolSummarizer;
    /// use swiftide_core::SimplePrompt;
    /// fn attach(agent: &mut AgentBuilder, model: Arc<dyn SimplePrompt>) -> anyhow::Result<()> {
    ///     let hook = ToolSummarizer::builder().llm(model).tools_to_summarize(Vec::new())
    ///         .git_start_sha("HEAD").build()?;
    ///     agent.after_tool(hook.summarize_hook());
    ///     Ok(())
    /// }
    /// ```
    #[allow(clippy::needless_borrow, reason = "preserve the source hook body")]
    pub fn summarize_hook(self) -> impl AfterToolFn {
        let this = Arc::new(self);

        move |agent, tool_call, tool_output| {
            let this = Arc::clone(&this);
            let available_tools = agent.tools();

            if let Ok(output) = tool_output {
                // If output is past the truncation treshhold, we always truncate first
                //
                // It's possible that after truncation, the output is now below the summarization
                // cutoff, in which case we skip summarization.
                if this.should_truncate(tool_call, output) {
                    tracing::info!("[ToolSummarizer] Truncated tool output");
                    this.truncate_output(output);
                }

                if !this.should_summarize(&available_tools, tool_call, output) {
                    return Box::pin(async move { Ok(()) });
                }

                let Some(tool) = available_tools
                    .iter()
                    .find(|tool| tool.name() == tool_call.name())
                else {
                    tracing::warn!(
                        "Tool {} not found in available tools, skipping summarization",
                        tool_call.name()
                    );
                    return Box::pin(async move { Ok(()) });
                };

                let span = tracing::info_span!("summarize_tool", tool = tool.name().as_ref());
                let prompt = prompt(
                    tool,
                    tool_call,
                    output,
                    available_tools,
                    this.summarize_all_at_token_cutoff,
                );
                let tool = tool.clone();

                return Box::pin(
                    async move {
                        let maybe_diff = this.maybe_diff(agent, &tool).await;
                        let prompt = prompt.with_context_value("diff", maybe_diff);

                        let summary = this.llm.prompt(prompt).await?;
                        tracing::debug!(summary = %summary, original = %output, "Summarized tool output");
                        *output = match output {
                            ToolOutput::Fail(_) => ToolOutput::fail(summary),
                            _ => summary.into(),
                        };

                        Ok(())
                    }
                    .instrument(span),
                );
            }
            Box::pin(async move { Ok(()) })
        }
    }

    fn should_summarize(
        &self,
        available_tools: &HashSet<Box<dyn Tool>>,
        tool_call: &ToolCall,
        tool_output: &ToolOutput,
    ) -> bool {
        // Tool is always expected to be summarized
        // Always double check that the tool is also available to the agent
        if self
            .tools_to_summarize
            .iter()
            .find(|t| *t == &tool_call.name())
            .and_then(|t| available_tools.iter().find(|tool| &tool.name() == t))
            .is_some()
        {
            return true;
        }

        // Any tools that are blacklisted should not be summarized
        if self
            .skip_summarize_all
            .as_ref()
            .is_some_and(|tools| tools.contains(&tool_call.name()))
        {
            return false;
        }

        if let Some(cutoff) = self.summarize_all_at_token_cutoff
            && tool_output
                .content()
                .is_some_and(|text| estimate_tokens(text) > cutoff)
        {
            // If the tool output is larger than the cutoff, summarize it
            return true;
        }

        false
    }

    fn should_truncate(&self, tool_call: &ToolCall, tool_output: &ToolOutput) -> bool {
        let Some(cutoff) = self.truncate_tokens_cutoff else {
            return false;
        };

        let Some(content) = tool_output.content() else {
            return false;
        };

        if self
            .skip_summarize_all
            .as_ref()
            .is_some_and(|tools| tools.contains(&tool_call.name()))
        {
            return false;
        }

        estimate_tokens(content) > cutoff
    }

    fn truncate_output(&self, tool_output: &mut ToolOutput) {
        let (ToolOutput::Text(content) | ToolOutput::Fail(content)) = tool_output else {
            return;
        };

        let cutoff_tokens = self
            .truncate_tokens_cutoff
            .unwrap_or(DEFAULT_TRUNCATE_TOKEN_CUTOFF);

        // Walk the lines lazily instead of collecting them; tool outputs can be very large.
        let line_count = content.lines().count();
        if line_count == 0 {
            return;
        }

        let head_len = self.truncate_head_lines.min(line_count);
        let tail_len = self
            .truncate_tail_lines
            .min(line_count.saturating_sub(head_len));

        // Prefer line-based truncation when there are enough lines to keep head + tail +
        // placeholder.
        if head_len + tail_len < line_count {
            let mut truncated = String::new();

            for (idx, line) in content.lines().take(head_len).enumerate() {
                truncated.push_str(line);
                if idx + 1 < head_len || tail_len > 0 {
                    truncated.push('\n');
                }
            }

            truncated.push_str(TRUNCATION_PLACEHOLDER);
            if tail_len > 0 {
                truncated.push('\n');
            }

            for (idx, line) in content.lines().skip(line_count - tail_len).enumerate() {
                truncated.push_str(line);
                if idx + 1 < tail_len {
                    truncated.push('\n');
                }
            }

            let truncated = self
                .truncate_to_char_budget(&truncated, cutoff_tokens)
                .unwrap_or(truncated);
            *content = truncated;
            return;
        }

        // Fallback: line-based truncation is ineffective (e.g., single huge line).
        // Use character-based truncation to ensure the output is reduced.
        if let Some(truncated) = self.truncate_to_char_budget(content, cutoff_tokens) {
            *content = truncated;
        }
    }

    /// Returns `None` when `text` already fits the budget.
    #[expect(clippy::unused_self)]
    fn truncate_to_char_budget(&self, text: &str, cutoff_tokens: usize) -> Option<String> {
        let placeholder = TRUNCATION_PLACEHOLDER;
        let char_budget = cutoff_tokens.saturating_mul(4); // align with estimate_tokens heuristic

        // Stops counting as soon as the budget is exceeded.
        text.chars().nth(char_budget)?;

        if char_budget <= placeholder.len() {
            return Some(placeholder.chars().take(char_budget).collect());
        }

        let available = char_budget - placeholder.len();
        let head_chars = available / 2;
        let tail_chars = available - head_chars;

        let head_end = text
            .char_indices()
            .nth(head_chars)
            .map_or(text.len(), |(idx, _)| idx);
        let tail_start = text
            .char_indices()
            .rev()
            .take(tail_chars)
            .last()
            .map_or(text.len(), |(idx, _)| idx);

        let mut truncated = String::with_capacity(char_budget);
        truncated.push_str(&text[..head_end]);
        truncated.push_str(placeholder);
        truncated.push_str(&text[tail_start..]);

        Some(truncated)
    }

    async fn maybe_diff(self: &Arc<Self>, agent: &Agent, tool: &dyn Tool) -> Option<String> {
        if self
            .tools_needing_diff
            .as_ref()
            .is_some_and(|tools| tools.contains(&tool.name().as_ref()))
        {
            let git_start_sha = &self.git_start_sha;

            let diff = accept_non_zero_exit(
                agent
                    .context()
                    .executor()
                    .exec_cmd(&Command::shell(format!(
                        "git diff {git_start_sha} --no-color"
                    )))
                    .await,
            )
            .ok()?
            .into_string_lossy();

            if diff.is_empty() { None } else { Some(diff) }
        } else {
            None
        }
    }
}

fn estimate_tokens(text: &str) -> usize {
    // Same heuristic as ContextCompression: roughly 4 characters per token.
    text.chars().count() / 4
}

fn prompt(
    tool: &dyn Tool,
    tool_call: &ToolCall,
    tool_output: &ToolOutput,
    available_tools: &HashSet<Box<dyn Tool>>,
    summarize_all_at_token_cutoff: Option<usize>,
) -> Prompt {
    // TODO: Partial?
    let formatted_tools = available_tools
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

    // TODO: Prompt should use quak's tera repository
    let template: Prompt = Templates::from_file("tool_summarizer.md")
        .expect("Infallible; failed to find template")
        .into();

    // If the output is larger than the cutoff, regardless of the tool, add prompt to summarize
    // instead of reformat
    let prompt_to_summarize = tool_output.content().is_some_and(|text| {
        estimate_tokens(text)
            > summarize_all_at_token_cutoff.unwrap_or(DEFAULT_SUMMARIZE_TOKENS_CUTOFF)
    });

    template
        .with_context_value("formatted_tools", formatted_tools)
        .with_context_value("tool_name", tool.name())
        .with_context_value("tool_description", tool.tool_spec().description)
        .with_context_value("tool_args", tool_call.args())
        .with_context_value("tool_output", tool_output.content())
        .with_context_value("prompt_to_summarize", prompt_to_summarize)
}

impl ToolSummarizerBuilder {
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
    /// use swiftide_agents_contrib::hooks::ToolSummarizer;
    /// use swiftide_core::SimplePrompt;
    /// fn attach(agent: &mut AgentBuilder, model: Arc<dyn SimplePrompt>) -> anyhow::Result<()> {
    ///     let hook = ToolSummarizer::builder().llm(model).tools_to_summarize(Vec::new())
    ///         .git_start_sha("HEAD").build()?;
    ///     agent.after_tool(hook.summarize_hook());
    ///     Ok(())
    /// }
    /// ```
    pub fn build(&self) -> Result<ToolSummarizer, ToolSummarizerBuilderError> {
        self.build_inner()
    }
}
