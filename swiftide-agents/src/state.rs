//! Internal state of the agent

use swiftide_core::chat_completion::ToolCall;

#[derive(Clone, Debug, Default, strum_macros::EnumDiscriminants, strum_macros::EnumIs)]
pub(crate) enum State {
    #[default]
    Pending,
    Running,
    #[allow(dead_code)]
    Stopped(StopReason),
}

/// The reason the agent stopped
///
/// `StopReason::Other` has some convenience methods to convert from any `AsRef<str>`
///
/// A default is also provided for `StopReason`
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum StopReason {
    RequestedByTool(ToolCall),
    ToolCallsOverLimit(ToolCall),
    Error,
    NoNewMessages,
    Other(String),
}

impl Default for StopReason {
    fn default() -> Self {
        StopReason::Other("No reason provided".to_string())
    }
}

impl<S: AsRef<str>> From<S> for StopReason {
    fn from(value: S) -> Self {
        StopReason::Other(value.as_ref().to_string())
    }
}
