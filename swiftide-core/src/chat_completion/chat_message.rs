use serde::{Deserialize, Serialize};

use crate::chat_completion::Usage;

use super::tools::{ToolCall, ToolOutput};

/// Simplified representation of messages for agents
#[derive(Clone, strum_macros::EnumIs, PartialEq, Debug, Serialize, Deserialize)]
pub enum ChatMessage {
    System(String),
    User(String),
    // An assistant can have a message, zero or more tool calls, and usage information
    Assistant(Option<String>, Option<Vec<ToolCall>>, Option<Usage>),
    ToolOutput(ToolCall, ToolOutput),

    // A summary of the chat. If encountered all previous messages are ignored, except the system
    // prompt
    Summary(String),
}

impl std::fmt::Display for ChatMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatMessage::System(s) => write!(f, "System: \"{s}\""),
            ChatMessage::User(s) => write!(f, "User: \"{s}\""),
            ChatMessage::Assistant(message, tool_calls, usage) => write!(
                f,
                "Assistant: \"{}\", tools: {} usage (input,output,total): {}",
                message.as_deref().unwrap_or("None"),
                tool_calls.as_deref().map_or("None".to_string(), |tc| {
                    tc.iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                }),
                if let Some(u) = usage {
                    format!(
                        "({}, {}, {})",
                        u.prompt_tokens, u.completion_tokens, u.total_tokens
                    )
                } else {
                    "None".to_string()
                }
            ),
            ChatMessage::ToolOutput(tc, to) => write!(f, "ToolOutput: \"{tc}\": \"{to}\""),
            ChatMessage::Summary(s) => write!(f, "Summary: \"{s}\""),
        }
    }
}

impl ChatMessage {
    pub fn new_system(message: impl Into<String>) -> Self {
        ChatMessage::System(message.into())
    }

    pub fn new_user(message: impl Into<String>) -> Self {
        ChatMessage::User(message.into())
    }

    pub fn new_assistant(
        message: Option<impl Into<String>>,
        tool_calls: Option<Vec<ToolCall>>,
        usage: Option<Usage>,
    ) -> Self {
        ChatMessage::Assistant(message.map(Into::into), tool_calls, usage)
    }

    pub fn new_tool_output(tool_call: impl Into<ToolCall>, output: impl Into<ToolOutput>) -> Self {
        ChatMessage::ToolOutput(tool_call.into(), output.into())
    }

    pub fn new_summary(message: impl Into<String>) -> Self {
        ChatMessage::Summary(message.into())
    }
}
