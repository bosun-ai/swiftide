use serde::{Deserialize, Serialize};

use super::tools::{ToolCall, ToolOutput};

/// Reasoning items returned by chat providers that expose chain-of-thought metadata.
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

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatMessageContentSource {
    Url {
        url: String,
    },
    Bytes {
        data: Vec<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        media_type: Option<String>,
    },
    S3 {
        uri: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bucket_owner: Option<String>,
    },
    FileId {
        file_id: String,
    },
}

impl ChatMessageContentSource {
    pub fn url(url: impl Into<String>) -> Self {
        Self::Url { url: url.into() }
    }

    pub fn bytes(data: impl Into<Vec<u8>>, media_type: Option<String>) -> Self {
        Self::Bytes {
            data: data.into(),
            media_type,
        }
    }

    pub fn s3(uri: impl Into<String>, bucket_owner: Option<String>) -> Self {
        Self::S3 {
            uri: uri.into(),
            bucket_owner,
        }
    }

    pub fn file_id(file_id: impl Into<String>) -> Self {
        Self::FileId {
            file_id: file_id.into(),
        }
    }
}

impl From<String> for ChatMessageContentSource {
    fn from(value: String) -> Self {
        Self::Url { url: value }
    }
}

impl From<&str> for ChatMessageContentSource {
    fn from(value: &str) -> Self {
        Self::Url {
            url: value.to_owned(),
        }
    }
}

impl From<Vec<u8>> for ChatMessageContentSource {
    fn from(value: Vec<u8>) -> Self {
        Self::Bytes {
            data: value,
            media_type: None,
        }
    }
}

impl std::fmt::Debug for ChatMessageContentSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatMessageContentSource::Url { url } => f
                .debug_struct("Url")
                .field("url", &truncate_data_url(url))
                .finish(),
            ChatMessageContentSource::Bytes { data, media_type } => f
                .debug_struct("Bytes")
                .field("len", &data.len())
                .field("media_type", media_type)
                .finish(),
            ChatMessageContentSource::S3 { uri, bucket_owner } => f
                .debug_struct("S3")
                .field("uri", uri)
                .field("bucket_owner", bucket_owner)
                .finish(),
            ChatMessageContentSource::FileId { file_id } => {
                f.debug_struct("FileId").field("file_id", file_id).finish()
            }
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatMessageContentPart {
    Text {
        text: String,
    },
    Image {
        source: ChatMessageContentSource,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        format: Option<String>,
    },
    Document {
        source: ChatMessageContentSource,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        format: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    Audio {
        source: ChatMessageContentSource,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        format: Option<String>,
    },
    Video {
        source: ChatMessageContentSource,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        format: Option<String>,
    },
}

impl ChatMessageContentPart {
    pub fn text(text: impl Into<String>) -> Self {
        ChatMessageContentPart::Text { text: text.into() }
    }

    pub fn image(source: impl Into<ChatMessageContentSource>) -> Self {
        ChatMessageContentPart::Image {
            source: source.into(),
            format: None,
        }
    }

    pub fn image_with_format(
        source: impl Into<ChatMessageContentSource>,
        format: impl Into<String>,
    ) -> Self {
        ChatMessageContentPart::Image {
            source: source.into(),
            format: Some(format.into()),
        }
    }

    pub fn document(source: impl Into<ChatMessageContentSource>) -> Self {
        ChatMessageContentPart::Document {
            source: source.into(),
            format: None,
            name: None,
        }
    }

    pub fn document_with_name(
        source: impl Into<ChatMessageContentSource>,
        name: impl Into<String>,
    ) -> Self {
        ChatMessageContentPart::Document {
            source: source.into(),
            format: None,
            name: Some(name.into()),
        }
    }

    pub fn audio(source: impl Into<ChatMessageContentSource>) -> Self {
        ChatMessageContentPart::Audio {
            source: source.into(),
            format: None,
        }
    }

    pub fn video(source: impl Into<ChatMessageContentSource>) -> Self {
        ChatMessageContentPart::Video {
            source: source.into(),
            format: None,
        }
    }
}

impl std::fmt::Debug for ChatMessageContentPart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatMessageContentPart::Text { text } => {
                f.debug_struct("Text").field("text", text).finish()
            }
            ChatMessageContentPart::Image { source, format } => f
                .debug_struct("Image")
                .field("source", source)
                .field("format", format)
                .finish(),
            ChatMessageContentPart::Document {
                source,
                format,
                name,
            } => f
                .debug_struct("Document")
                .field("source", source)
                .field("format", format)
                .field("name", name)
                .finish(),
            ChatMessageContentPart::Audio { source, format } => f
                .debug_struct("Audio")
                .field("source", source)
                .field("format", format)
                .finish(),
            ChatMessageContentPart::Video { source, format } => f
                .debug_struct("Video")
                .field("source", source)
                .field("format", format)
                .finish(),
        }
    }
}

#[derive(Clone, strum_macros::EnumIs, PartialEq, Debug, Serialize, Deserialize)]
pub enum ChatMessage {
    System(String),
    User(String),
    UserWithParts(Vec<ChatMessageContentPart>),
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
            ChatMessage::UserWithParts(parts) => {
                let (text, attachments) = summarize_user_parts(parts);
                if attachments == 0 {
                    write!(f, "User: \"{text}\"")
                } else {
                    write!(f, "User: \"{text}\", attachments: {attachments}")
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

    pub fn new_user(message: impl Into<String>) -> Self {
        ChatMessage::User(message.into())
    }

    pub fn new_user_with_parts(parts: impl Into<Vec<ChatMessageContentPart>>) -> Self {
        ChatMessage::UserWithParts(parts.into())
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
            ChatMessage::UserWithParts(parts) => match parts.as_slice() {
                [ChatMessageContentPart::Text { text }] => text.as_str(),
                _ => "",
            },
            ChatMessage::Assistant(message, _) => message.as_deref().unwrap_or(""),
            ChatMessage::ToolOutput(_, output) => output.content().unwrap_or(""),
            ChatMessage::Reasoning(_) => "",
        }
    }
}

fn summarize_user_parts(parts: &[ChatMessageContentPart]) -> (String, usize) {
    let mut text_parts = Vec::new();
    let mut attachments = 0;
    for part in parts {
        match part {
            ChatMessageContentPart::Text { text } => text_parts.push(text.as_str()),
            ChatMessageContentPart::Image { .. }
            | ChatMessageContentPart::Document { .. }
            | ChatMessageContentPart::Audio { .. }
            | ChatMessageContentPart::Video { .. } => attachments += 1,
        }
    }
    (text_parts.join(" "), attachments)
}

fn truncate_data_url(url: &str) -> std::borrow::Cow<'_, str> {
    const MAX_DATA_PREVIEW: usize = 32;

    if !url.starts_with("data:") {
        return std::borrow::Cow::Borrowed(url);
    }

    let Some((prefix, data)) = url.split_once(',') else {
        return std::borrow::Cow::Borrowed(url);
    };

    if data.len() <= MAX_DATA_PREVIEW {
        return std::borrow::Cow::Borrowed(url);
    }

    let preview = &data[..MAX_DATA_PREVIEW];
    let truncated = data.len() - MAX_DATA_PREVIEW;

    std::borrow::Cow::Owned(format!(
        "{prefix},{preview}...[truncated {truncated} chars]"
    ))
}
