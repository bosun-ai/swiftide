//! Tool-output summarization and conversation compression hooks.
//!
//! Register tool summaries with `after_tool` and conversation compression with `register_hooks`.
//! Supply your own model and Git revision.
//!
//! # Examples
//!
//! ```no_run
//! use std::sync::Arc;
//! use swiftide_agents::AgentBuilder;
//! use swiftide_agents_contrib::hooks::ContextCompression;
//! use swiftide_core::ChatCompletion;
//! fn attach(agent: &mut AgentBuilder, model: Arc<dyn ChatCompletion>) -> anyhow::Result<()> {
//!     let compression = ContextCompression::builder().llm(model)
//!         .git_start_sha("HEAD").git_diff_dir(".").build()?;
//!     compression.register_hooks(agent);
//!     Ok(())
//! }
//! ```

mod context_compression;
mod tool_summarizer;
pub use context_compression::{ContextCompression, ContextCompressionBuilder};
pub use tool_summarizer::{ToolSummarizer, ToolSummarizerBuilder};
