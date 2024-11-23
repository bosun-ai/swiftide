use super::tools::{ToolCall, ToolOutput};

#[derive(Clone, strum_macros::EnumIs, PartialEq, Debug)]
pub enum ChatMessage {
    System(String),
    User(String),
    Assistant(String),
    ToolCall(ToolCall),
    ToolOutput(ToolCall, ToolOutput),
}

impl std::fmt::Display for ChatMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatMessage::System(s) => write!(f, "System: \"{s}\""),
            ChatMessage::User(s) => write!(f, "User: \"{s}\""),
            ChatMessage::Assistant(s) => write!(f, "Assistant: \"{s}\""),
            ChatMessage::ToolCall(tc) => write!(f, "ToolCall: \"{tc}\""),
            ChatMessage::ToolOutput(tc, to) => write!(f, "ToolOutput: \"{tc}\": \"{to}\""),
        }
    }
}
