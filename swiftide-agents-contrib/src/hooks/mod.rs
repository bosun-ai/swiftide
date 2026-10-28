//! Context hooks for tool results and long conversations.
//!
//! Register [`ToolSummarizer::summarize_hook`] with [`swiftide_agents::AgentBuilder::after_tool`]
//! and add every [`ContextCompression::hooks`] entry with
//! [`swiftide_agents::AgentBuilder::add_hook`]. Both hooks retain usable input when a model
//! fails.
//!
//! # Examples
//!
//! Keep a long coding session's logs and conversation small enough to use:
//!
//! ```no_run
//! use std::sync::Arc;
//! use swiftide_agents::AgentBuilder;
//! use swiftide_agents_contrib::hooks::{ContextCompression, ToolSummarizer};
//! use swiftide_core::ChatCompletion;
//!
//! fn manage_session_context(
//!     agent: &mut AgentBuilder,
//!     summary_model: Arc<dyn ChatCompletion>,
//! ) -> anyhow::Result<()> {
//!     let logs = ToolSummarizer::builder()
//!         .max_output_chars(20_000)
//!         .excluded_tools(["read_file", "read_file_with_line_numbers"])
//!         .build()?;
//!     let conversation = ContextCompression::builder()
//!         .llm(summary_model)
//!         .context_limit(32_000)
//!         .build()?;
//!     agent.after_tool(logs.summarize_hook());
//!     for hook in conversation.hooks() {
//!         agent.add_hook(hook);
//!     }
//!     Ok(())
//! }
//! ```

mod context_compression;
mod tool_summarizer;
pub use context_compression::{ContextCompression, ContextCompressionBuilder};
pub use tool_summarizer::{ToolSummarizer, ToolSummarizerBuilder};
