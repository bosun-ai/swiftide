use serde::{Deserialize, Serialize};

use super::tools::{ToolCall, ToolOutput};

/// Reasoning items returned by the Responses API (openai specific)
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, Default)]
pub struct ReasoningItem {
    /// Unique identifier for this reasoning item
    pub id: String,
    /// Reasoning summary content
    pub summary: Vec<String>,
    /// Reasoning text content
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypted_content: Option<String>,
    /// The status of the item. One of `in_progress`, `completed`, or `incomplete`.
    /// Populated when items are returned via API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ReasoningStatus>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningStatus {
    InProgress,
    Completed,
    Incomplete,
}

#[derive(Clone, strum_macros::EnumIs, PartialEq, Debug, Serialize, Deserialize)]
pub enum ChatMessage {
    System(String),
    User(String),
    Assistant(Option<String>, Option<Vec<ToolCall>>),
    ToolOutput(ToolCall, ToolOutput),
    Reasoning(ReasoningItem),

    // A summary of the chat. If encountered all previous messages are ignored, except the system
    // prompt
    Summary(String),
}

impl std::fmt::Display for ChatMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatMessage::System(s) => write!(f, "System: \"{s}\""),
            ChatMessage::User(s) => write!(f, "User: \"{s}\""),
            ChatMessage::Assistant(content, tool_calls) => write!(
                f,
                "Assistant: \"{}\", tools: {}",
                content.as_deref().unwrap_or("None"),
                tool_calls.as_deref().map_or("None".to_string(), |tc| {
                    tc.iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
            ),
            ChatMessage::ToolOutput(tc, to) => write!(f, "ToolOutput: \"{tc}\": \"{to}\""),
            ChatMessage::Reasoning(item) => write!(
                f,
                "Reasoning: \"{}\", encrypted: {}",
                item.summary.join("\n"),
                item.encrypted_content.is_some()
            ),
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
    ) -> Self {
        ChatMessage::Assistant(message.map(Into::into), tool_calls)
    }

    pub fn new_tool_output(tool_call: impl Into<ToolCall>, output: impl Into<ToolOutput>) -> Self {
        ChatMessage::ToolOutput(tool_call.into(), output.into())
    }

    pub fn new_reasoning(message: ReasoningItem) -> Self {
        ChatMessage::Reasoning(message)
    }

    pub fn new_summary(message: impl Into<String>) -> Self {
        ChatMessage::Summary(message.into())
    }
}

/// Returns the content of the message as a string slice.
///
/// Note that this omits the tool calls from the assistant message.
///
/// If used for estimating tokens, consider this a very rought estimate
impl AsRef<str> for ChatMessage {
    fn as_ref(&self) -> &str {
        match self {
            ChatMessage::System(s) | ChatMessage::User(s) | ChatMessage::Summary(s) => s,
            ChatMessage::Assistant(message, _) => message.as_deref().unwrap_or(""),
            ChatMessage::ToolOutput(_, output) => output.content().unwrap_or(""),
            ChatMessage::Reasoning(_) => "",
        }
    }
}
