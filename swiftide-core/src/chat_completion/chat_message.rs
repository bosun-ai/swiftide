use serde::{Deserialize, Serialize};
use std::borrow::Cow;

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

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ImageDetail {
    #[default]
    Auto,
    Low,
    High,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatMessageContentPart {
    Text { text: String },
    ImageUrl {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<ImageDetail>,
    },
}

impl ChatMessageContentPart {
    pub fn text(text: impl Into<String>) -> Self {
        ChatMessageContentPart::Text { text: text.into() }
    }

    pub fn image_url(url: impl Into<String>, detail: Option<ImageDetail>) -> Self {
        ChatMessageContentPart::ImageUrl {
            url: url.into(),
            detail,
        }
    }

    fn is_image(&self) -> bool {
        matches!(self, ChatMessageContentPart::ImageUrl { .. })
    }

    fn text_ref(&self) -> Option<&str> {
        match self {
            ChatMessageContentPart::Text { text } => Some(text.as_str()),
            ChatMessageContentPart::ImageUrl { .. } => None,
        }
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChatMessageContent {
    Text(String),
    Parts(Vec<ChatMessageContentPart>),
}

impl ChatMessageContent {
    pub fn text(message: impl Into<String>) -> Self {
        ChatMessageContent::Text(message.into())
    }

    pub fn parts(parts: impl Into<Vec<ChatMessageContentPart>>) -> Self {
        ChatMessageContent::Parts(parts.into())
    }

    pub fn has_images(&self) -> bool {
        match self {
            ChatMessageContent::Text(_) => false,
            ChatMessageContent::Parts(parts) => parts.iter().any(ChatMessageContentPart::is_image),
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            ChatMessageContent::Text(text) => Some(text.as_str()),
            ChatMessageContent::Parts(parts) => match parts.as_slice() {
                [ChatMessageContentPart::Text { text }] => Some(text.as_str()),
                _ => None,
            },
        }
    }

    pub fn text_fragments(&self) -> Vec<Cow<'_, str>> {
        match self {
            ChatMessageContent::Text(text) => vec![Cow::Borrowed(text.as_str())],
            ChatMessageContent::Parts(parts) => parts
                .iter()
                .filter_map(ChatMessageContentPart::text_ref)
                .map(Cow::Borrowed)
                .collect(),
        }
    }

    fn summary(&self) -> (String, usize) {
        match self {
            ChatMessageContent::Text(text) => (text.clone(), 0),
            ChatMessageContent::Parts(parts) => {
                let mut text_parts = Vec::new();
                let mut images = 0;
                for part in parts {
                    match part {
                        ChatMessageContentPart::Text { text } => text_parts.push(text.as_str()),
                        ChatMessageContentPart::ImageUrl { .. } => images += 1,
                    }
                }
                (text_parts.join(" "), images)
            }
        }
    }
}

impl From<String> for ChatMessageContent {
    fn from(value: String) -> Self {
        ChatMessageContent::Text(value)
    }
}

impl From<&str> for ChatMessageContent {
    fn from(value: &str) -> Self {
        ChatMessageContent::Text(value.to_string())
    }
}

impl From<Vec<ChatMessageContentPart>> for ChatMessageContent {
    fn from(value: Vec<ChatMessageContentPart>) -> Self {
        ChatMessageContent::Parts(value)
    }
}

#[derive(Clone, strum_macros::EnumIs, PartialEq, Debug, Serialize, Deserialize)]
pub enum ChatMessage {
    System(String),
    User(ChatMessageContent),
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
            ChatMessage::User(content) => {
                let (text, images) = content.summary();
                if images == 0 {
                    write!(f, "User: \"{text}\"")
                } else {
                    write!(f, "User: \"{text}\", images: {images}")
                }
            }
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

    pub fn new_user(message: impl Into<ChatMessageContent>) -> Self {
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
            ChatMessage::System(s) | ChatMessage::Summary(s) => s,
            ChatMessage::User(content) => content.as_text().unwrap_or(""),
            ChatMessage::Assistant(message, _) => message.as_deref().unwrap_or(""),
            ChatMessage::ToolOutput(_, output) => output.content().unwrap_or(""),
            ChatMessage::Reasoning(_) => "",
        }
    }
}
