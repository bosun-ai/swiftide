//! Build and run typed task graphs.
//!
//! Tasks connect [`TaskNode`] implementations into a small execution graph. Each node receives a
//! typed input, produces a typed output, and hands control to the next step through a
//! [`Transition`].
//!
//! The main entry points are:
//! - [`Task`] to define and run a graph
//! - [`TaskBuilder`] to configure execution defaults such as concurrency
//! - [`Transition`] to describe how execution should continue after a node finishes
//! - [`Task::register_node_fn`] for lightweight synchronous closure nodes
//! - [`NodeId`] helpers such as [`NodeId::transitions_with`] and [`NodeId::join`] for the common
//!   next-step and join cases
//! - [`Transition::fan_out`] for typed static fan-out branches
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
//! let join = task.register_node_fn(|input: &JoinInput<i32>| -> Result<i32, NodeError> {
//!     Ok(input.iter().copied().sum())
//! });
//!
//! task.starts_with(start);
//! task.register_transition(start, move |value| {
//!     // `join_with` defines the branch group and the join node that waits for it.
//!     Transition::fan_out(&double, value)
//!         .and(&increment, value)
//!         .join_with(join.join())
//! })?;
//! // Branches still decide where their own output goes before the join can run.
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
mod executor;
mod node;
mod runtime;
mod task;
mod traits;
mod transition;

pub use adapters::{AsyncFn, SyncFn, TaskAgent};
pub use errors::{NodeError, TaskError};
pub use node::NodeId;
pub use task::{Task, TaskBuilder, TaskRunState};
pub use traits::{DynNodeId, NodeArg, TaskNode};
pub use transition::{
    AnyJoinInput, AnyJoinTarget, ConcurrencyModel, FanOutTransition, JoinInput, JoinTarget,
    MappedJoinTarget, MarkedTransition, Transition,
};
