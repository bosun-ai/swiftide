//! File tools and shared plans for agent tool loops.
//!
//! Register each tool with your agent builder's `add_tool` method. The `file-tools` feature
//! provides file reading and editing; `planning` provides the in-memory plan tool and its state.
//!
//! # Examples
//!
//! Give an agent tools to read and edit project files:
//!
//! ```no_run
//! # #[cfg(feature = "file-tools")]
//! # {
//! use swiftide_agents::Agent;
//! use swiftide_agents_contrib::tools::{ApplyPatch, ExactReplace, ReadFile};
//!
//! Agent::builder()
//!     .add_tool(ReadFile::default())
//!     .add_tool(ExactReplace)
//!     .add_tool(ApplyPatch);
//! # }
//! ```
//!
//! Track the agent's progress in a shared plan:
//!
//! ```no_run
//! # #[cfg(feature = "planning")]
//! # {
//! use swiftide_agents::Agent;
//! use swiftide_agents_contrib::tools::UpdatePlan;
//!
//! Agent::builder().add_tool(UpdatePlan::default());
//! # }
//! ```

#[cfg(feature = "file-tools")]
mod apply_patch;
#[cfg(feature = "file-tools")]
mod exact_replace;
#[cfg(feature = "file-tools")]
mod read_file;
#[cfg(feature = "planning")]
mod update_plan;

#[cfg(feature = "file-tools")]
pub use apply_patch::ApplyPatch;
#[cfg(feature = "file-tools")]
pub use exact_replace::ExactReplace;
#[cfg(feature = "file-tools")]
pub use read_file::ReadFile;
#[cfg(feature = "planning")]
pub use update_plan::{PlanItem, PlanMemory, PlanStatus, UpdatePlan};
