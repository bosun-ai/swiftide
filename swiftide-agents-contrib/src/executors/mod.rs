//! Wrappers for caller-provided executors.
//!
//! # Examples
//!
//! Defer workspace setup until an agent first needs a command or a file:
//!
//! ```no_run
//! use std::sync::Arc;
//! use swiftide_agents::{Agent, DefaultContext, tools::local_executor::LocalExecutor};
//! use swiftide_agents_contrib::executors::LazyExecutor;
//! use swiftide_core::ToolExecutor;
//!
//! let executor = LazyExecutor::new(|| async {
//!     // Supply the application's executor setup here.
//!     Ok(Arc::new(LocalExecutor::default()) as Arc<dyn ToolExecutor>)
//! });
//! let executor: Arc<dyn ToolExecutor> = Arc::new(executor);
//! Agent::builder().context(DefaultContext::from_executor(executor));
//! ```

mod lazy;
pub use lazy::LazyExecutor;
