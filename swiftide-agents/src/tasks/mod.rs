//! Build and run typed task graphs.
//!
//! Tasks connect [`TaskNode`] implementations into a small execution graph. Each node receives a
//! typed input, produces a typed output, and hands control to the next step through a
//! [`Transition`].
//!
//! The main entry points are:
//! - [`Task`] to define and run a graph
//! - [`TaskBuilder`] to configure execution defaults such as concurrency and pause behavior
//! - [`Transition`] to describe how execution should continue after a node finishes
//! - [`Task::runtime_state`] and [`Task::restore_runtime_state`] to export and restore a paused
//!   runtime frontier
//! - [`Task::register_node_fn`] for lightweight synchronous closure nodes
//! - [`NodeId`] helpers such as [`NodeId::transitions_with`] and [`NodeId::join`] for the common
//!   next-step and join cases
//!
//! # Examples
//!
//! A small linear task:
//!
//! ```no_run
//! use swiftide_agents::tasks::{NodeError, Task, TaskRunState};
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut task = Task::<i32, i32>::new();
//!
//! let start = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 1) });
//! let finish =
//!     task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input * 2) });
//!
//! task.starts_with(start);
//! task.register_transition(start, move |value| finish.transitions_with(value))?;
//! task.register_transition(finish, task.transitions_to_finish())?;
//!
//! let result = task.run(2).await?;
//! assert_eq!(result, TaskRunState::Completed(6));
//! # Ok(())
//! # }
//! ```
//!
//! A fan-out with an explicit join:
//!
//! ```no_run
//! use swiftide_agents::tasks::{JoinInput, NodeError, Task, TaskRunState, Transition};
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut task = Task::<i32, i32>::new();
//!
//! let start = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input) });
//! let double =
//!     task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input * 2) });
//! let increment =
//!     task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 1) });
//! let join = task.register_node_fn(|input: &JoinInput| -> Result<i32, NodeError> {
//!     Ok(input.ready_values::<i32>().into_iter().copied().sum())
//! });
//!
//! task.starts_with(start);
//! task.register_transition(start, move |value| {
//!     Transition::fan_out([double.target_with(value), increment.target_with(value)])
//! })?;
//! task.register_transition(double, join.join())?;
//! task.register_transition(increment, join.join())?;
//! task.register_transition(join, task.transitions_to_finish())?;
//!
//! let result = task.run(3).await?;
//! assert_eq!(result, TaskRunState::Completed(10));
//! # Ok(())
//! # }
//! ```
mod adapters;
mod errors;
mod node;
mod runtime;
mod task;
mod transition;

pub use adapters::{AsyncFn, SyncFn, TaskAgent};
pub use errors::{NodeError, TaskError};
pub use node::{NodeArg, NodeId, TaskNode};
#[allow(deprecated)]
pub use task::{
    RestoredBranch, RestoredJoinGroup, RestoredJoinMember, RestoredJoinMemberOutcome,
    RuntimeBranchSettings, Task, TaskBuilder, TaskRunState, TaskRuntimeSeed, TaskRuntimeState,
};
pub use transition::{
    ActiveBranch, AsyncMappedJoinTarget, AtLeastJoin, BranchEnvelope, BranchId, BranchOutcome,
    ConcurrencyModel, ErrorBehavior, JoinInput, JoinLeftoverBehavior, JoinPolicy, JoinScope,
    JoinTarget, MappedJoinTarget, MarkedTransition, NextNode, PauseBehavior, Transition,
};
