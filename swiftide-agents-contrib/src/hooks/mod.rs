//! Context hooks for tool results and long conversations.
//!
//! Register [`ToolSummarizer::summarize_hook`] with [`swiftide_agents::AgentBuilder::after_tool`]
//! and [`ContextCompression::summarize_hook`] with
//! [`swiftide_agents::AgentBuilder::after_each`]. Both hooks retain usable input when a model
//! fails.

mod context_compression;
mod tool_summarizer;
pub use context_compression::{ContextCompression, ContextCompressionBuilder};
pub use tool_summarizer::{ToolSummarizer, ToolSummarizerBuilder};
