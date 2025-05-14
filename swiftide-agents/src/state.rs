//! Internal state of the agent

use swiftide_core::chat_completion::ToolCall;

#[derive(Clone, Debug, Default, strum_macros::EnumDiscriminants, strum_macros::EnumIs)]
pub enum State {
    #[default]
    Pending,
    Running,
    Stopped(StopReason),
}

impl State {
    pub fn stop_reason(&self) -> Option<&StopReason> {
        match self {
            State::Stopped(reason) => Some(reason),
            _ => None,
        }
    }
}

/// The reason the agent stopped
///
/// `StopReason::Other` has some convenience methods to convert from any `AsRef<str>`
#[non_exhaustive]
#[derive(Clone, Debug, strum_macros::EnumIs)]
pub enum StopReason {
    /// A tool called stop
    RequestedByTool(ToolCall),

    /// A tool repeatedly failed
    ToolCallsOverLimit(ToolCall),

    /// A tool requires feedback before it will continue
    FeedbackRequired {
        tool_call: ToolCall,
        payload: Option<serde_json::Value>,
    },
    /// There was an error
    Error,

    /// No new messages; stopping completions
    NoNewMessages,

    Other(String),
}

impl StopReason {
    pub fn requested_by_tool(&self) -> Option<&ToolCall> {
        if let StopReason::RequestedByTool(t) = self {
            Some(t)
        } else {
            None
        }
    }

    pub fn tool_calls_over_limit(&self) -> Option<&ToolCall> {
        if let StopReason::ToolCallsOverLimit(t) = self {
            Some(t)
        } else {
            None
        }
    }

    pub fn feedback_required(&self) -> Option<(&ToolCall, Option<&serde_json::Value>)> {
        if let StopReason::FeedbackRequired { tool_call, payload } = self {
            Some((tool_call, payload.as_ref()))
        } else {
            None
        }
    }

    pub fn error(&self) -> Option<()> {
        if matches!(self, StopReason::Error) {
            Some(())
        } else {
            None
        }
    }

    pub fn no_new_messages(&self) -> Option<()> {
        if matches!(self, StopReason::NoNewMessages) {
            Some(())
        } else {
            None
        }
    }

    pub fn other(&self) -> Option<&str> {
        if let StopReason::Other(s) = self {
            Some(s)
        } else {
            None
        }
    }
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
