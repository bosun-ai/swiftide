//! Task reexports for agent users.
//!
//! This module keeps the existing `swiftide_agents::tasks` and `swiftide::agents::tasks` paths
//! available while task graph primitives live in the `swiftide-tasks` crate. It also exposes
//! [`TaskAgent`], the adapter that wraps an [`Agent`](crate::Agent) as a task node.

#[doc(inline)]
pub use swiftide_tasks::*;

#[doc(inline)]
pub use crate::task_agent::TaskAgent;
