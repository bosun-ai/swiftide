use std::borrow::Cow;

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

pub type ChatMessageContentSource = ChatMessageContentSourceValue<'static>;
pub type ChatMessageContentPart = ChatMessageContentPartValue<'static>;
pub type ChatMessage = ChatMessageValue<'static>;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatMessageContentSourceValue<'a> {
    Url {
        url: Cow<'a, str>,
    },
    Bytes {
        data: Cow<'a, [u8]>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        media_type: Option<Cow<'a, str>>,
    },
    S3 {
        uri: Cow<'a, str>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bucket_owner: Option<Cow<'a, str>>,
    },
    FileId {
        file_id: Cow<'a, str>,
    },
}

impl<'a> ChatMessageContentSourceValue<'a> {
    pub fn url(url: impl Into<Cow<'a, str>>) -> Self {
        Self::Url { url: url.into() }
    }

    pub fn bytes<M>(data: impl Into<Cow<'a, [u8]>>, media_type: Option<M>) -> Self
    where
        M: Into<Cow<'a, str>>,
    {
        Self::Bytes {
            data: data.into(),
            media_type: media_type.map(Into::into),
        }
    }

    pub fn s3<O>(uri: impl Into<Cow<'a, str>>, bucket_owner: Option<O>) -> Self
    where
        O: Into<Cow<'a, str>>,
    {
        Self::S3 {
            uri: uri.into(),
            bucket_owner: bucket_owner.map(Into::into),
        }
    }

    pub fn file_id(file_id: impl Into<Cow<'a, str>>) -> Self {
        Self::FileId {
            file_id: file_id.into(),
        }
    }
}

impl From<String> for ChatMessageContentSourceValue<'static> {
    fn from(value: String) -> Self {
        Self::Url {
            url: Cow::Owned(value),
        }
    }
}

impl<'a> From<Cow<'a, str>> for ChatMessageContentSourceValue<'a> {
    fn from(value: Cow<'a, str>) -> Self {
        Self::Url { url: value }
    }
}

impl<'a> From<&'a str> for ChatMessageContentSourceValue<'a> {
    fn from(value: &'a str) -> Self {
        Self::Url {
            url: Cow::Borrowed(value),
        }
    }
}

impl From<Vec<u8>> for ChatMessageContentSourceValue<'static> {
    fn from(value: Vec<u8>) -> Self {
        Self::Bytes {
            data: Cow::Owned(value),
            media_type: None,
        }
    }
}

impl<'a> From<Cow<'a, [u8]>> for ChatMessageContentSourceValue<'a> {
    fn from(value: Cow<'a, [u8]>) -> Self {
        Self::Bytes {
            data: value,
            media_type: None,
        }
    }
}

impl<'a> From<&'a [u8]> for ChatMessageContentSourceValue<'a> {
    fn from(value: &'a [u8]) -> Self {
        Self::Bytes {
            data: Cow::Borrowed(value),
            media_type: None,
        }
    }
}

impl std::fmt::Debug for ChatMessageContentSourceValue<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatMessageContentSourceValue::Url { url } => f
                .debug_struct("Url")
                .field("url", &truncate_data_url(url))
                .finish(),
            ChatMessageContentSourceValue::Bytes { data, media_type } => f
                .debug_struct("Bytes")
                .field("len", &data.len())
                .field("media_type", media_type)
                .finish(),
            ChatMessageContentSourceValue::S3 { uri, bucket_owner } => f
                .debug_struct("S3")
                .field("uri", uri)
                .field("bucket_owner", bucket_owner)
                .finish(),
            ChatMessageContentSourceValue::FileId { file_id } => {
                f.debug_struct("FileId").field("file_id", file_id).finish()
            }
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatMessageContentPartValue<'a> {
    Text {
        text: Cow<'a, str>,
    },
    Image {
        source: ChatMessageContentSourceValue<'a>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        format: Option<Cow<'a, str>>,
    },
    Document {
        source: ChatMessageContentSourceValue<'a>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        format: Option<Cow<'a, str>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<Cow<'a, str>>,
    },
    Audio {
        source: ChatMessageContentSourceValue<'a>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        format: Option<Cow<'a, str>>,
    },
    Video {
        source: ChatMessageContentSourceValue<'a>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        format: Option<Cow<'a, str>>,
    },
}

impl<'a> ChatMessageContentPartValue<'a> {
    pub fn text(text: impl Into<Cow<'a, str>>) -> Self {
        Self::Text { text: text.into() }
    }

    pub fn image(source: impl Into<ChatMessageContentSourceValue<'a>>) -> Self {
        Self::Image {
            source: source.into(),
            format: None,
        }
    }

    pub fn image_with_format(
        source: impl Into<ChatMessageContentSourceValue<'a>>,
        format: impl Into<Cow<'a, str>>,
    ) -> Self {
        Self::Image {
            source: source.into(),
            format: Some(format.into()),
        }
    }

    pub fn document(source: impl Into<ChatMessageContentSourceValue<'a>>) -> Self {
        Self::Document {
            source: source.into(),
            format: None,
            name: None,
        }
    }

    pub fn document_with_name(
        source: impl Into<ChatMessageContentSourceValue<'a>>,
        name: impl Into<Cow<'a, str>>,
    ) -> Self {
        Self::Document {
            source: source.into(),
            format: None,
            name: Some(name.into()),
        }
    }

    pub fn audio(source: impl Into<ChatMessageContentSourceValue<'a>>) -> Self {
        Self::Audio {
            source: source.into(),
            format: None,
        }
    }

    pub fn video(source: impl Into<ChatMessageContentSourceValue<'a>>) -> Self {
        Self::Video {
            source: source.into(),
            format: None,
        }
    }
}

impl std::fmt::Debug for ChatMessageContentPartValue<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatMessageContentPartValue::Text { text } => {
                f.debug_struct("Text").field("text", text).finish()
            }
            ChatMessageContentPartValue::Image { source, format } => f
                .debug_struct("Image")
                .field("source", source)
                .field("format", format)
                .finish(),
            ChatMessageContentPartValue::Document {
                source,
                format,
                name,
            } => f
                .debug_struct("Document")
                .field("source", source)
                .field("format", format)
                .field("name", name)
                .finish(),
            ChatMessageContentPartValue::Audio { source, format } => f
                .debug_struct("Audio")
                .field("source", source)
                .field("format", format)
                .finish(),
            ChatMessageContentPartValue::Video { source, format } => f
                .debug_struct("Video")
                .field("source", source)
                .field("format", format)
                .finish(),
        }
    }
}

#[derive(Clone, strum_macros::EnumIs, PartialEq, Debug, Serialize, Deserialize)]
pub enum ChatMessageValue<'a> {
    System(Cow<'a, str>),
    User(Cow<'a, str>),
    UserWithParts(Vec<ChatMessageContentPartValue<'a>>),
    Assistant(Option<Cow<'a, str>>, Option<Vec<ToolCall>>),
    ToolOutput(ToolCall, ToolOutput),
    Reasoning(ReasoningItem),

    // A summary of the chat. If encountered all previous messages are ignored, except the system
    // prompt
    Summary(Cow<'a, str>),
}

impl std::fmt::Display for ChatMessageValue<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::System(s) => write!(f, "System: \"{s}\""),
            Self::User(s) => write!(f, "User: \"{s}\""),
            Self::UserWithParts(parts) => {
                let (text, attachments) = summarize_user_parts(parts);
                if attachments == 0 {
                    write!(f, "User: \"{text}\"")
                } else {
                    write!(f, "User: \"{text}\", attachments: {attachments}")
                }
            }
            Self::Assistant(content, tool_calls) => write!(
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
            Self::ToolOutput(tc, to) => write!(f, "ToolOutput: \"{tc}\": \"{to}\""),
            Self::Reasoning(item) => write!(
                f,
                "Reasoning: \"{}\", encrypted: {}",
                item.summary.join("\n"),
                item.encrypted_content.is_some()
            ),
            Self::Summary(s) => write!(f, "Summary: \"{s}\""),
        }
    }
}

impl<'a> ChatMessageValue<'a> {
    pub fn new_system(message: impl Into<Cow<'a, str>>) -> Self {
        Self::System(message.into())
    }

    pub fn new_user(message: impl Into<Cow<'a, str>>) -> Self {
        Self::User(message.into())
    }

    pub fn new_user_with_parts(parts: impl Into<Vec<ChatMessageContentPartValue<'a>>>) -> Self {
        Self::UserWithParts(parts.into())
    }

    pub fn new_assistant(
        message: Option<impl Into<Cow<'a, str>>>,
        tool_calls: Option<Vec<ToolCall>>,
    ) -> Self {
        Self::Assistant(message.map(Into::into), tool_calls)
    }

    pub fn new_tool_output(tool_call: impl Into<ToolCall>, output: impl Into<ToolOutput>) -> Self {
        Self::ToolOutput(tool_call.into(), output.into())
    }

    pub fn new_reasoning(message: ReasoningItem) -> Self {
        Self::Reasoning(message)
    }

    pub fn new_summary(message: impl Into<Cow<'a, str>>) -> Self {
        Self::Summary(message.into())
    }

    pub fn to_owned(&self) -> ChatMessage {
        match self {
            Self::System(text) => ChatMessage::System(Cow::Owned(text.to_string())),
            Self::User(text) => ChatMessage::User(Cow::Owned(text.to_string())),
            Self::UserWithParts(parts) => ChatMessage::UserWithParts(
                parts
                    .iter()
                    .map(chat_message_content_part_to_owned)
                    .collect(),
            ),
            Self::Assistant(message, tool_calls) => ChatMessage::Assistant(
                message
                    .as_deref()
                    .map(|message| Cow::Owned(message.to_string())),
                tool_calls.clone(),
            ),
            Self::ToolOutput(tool_call, output) => {
                ChatMessage::ToolOutput(tool_call.clone(), output.clone())
            }
            Self::Reasoning(reasoning) => ChatMessage::Reasoning(reasoning.clone()),
            Self::Summary(text) => ChatMessage::Summary(Cow::Owned(text.to_string())),
        }
    }
}

/// Returns the content of the message as a string slice.
///
/// Note that this omits the tool calls from the assistant message.
///
/// If used for estimating tokens, consider this a very rought estimate
impl AsRef<str> for ChatMessageValue<'_> {
    fn as_ref(&self) -> &str {
        match self {
            Self::System(s) | Self::User(s) | Self::Summary(s) => s,
            Self::UserWithParts(parts) => match parts.as_slice() {
                [ChatMessageContentPartValue::Text { text }] => text.as_ref(),
                _ => "",
            },
            Self::Assistant(message, _) => message.as_deref().unwrap_or(""),
            Self::ToolOutput(_, output) => output.content().unwrap_or(""),
            Self::Reasoning(_) => "",
        }
    }
}

fn summarize_user_parts(parts: &[ChatMessageContentPartValue<'_>]) -> (String, usize) {
    let mut text_parts = Vec::new();
    let mut attachments = 0;
    for part in parts {
        match part {
            ChatMessageContentPartValue::Text { text } => text_parts.push(text.as_ref()),
            ChatMessageContentPartValue::Image { .. }
            | ChatMessageContentPartValue::Document { .. }
            | ChatMessageContentPartValue::Audio { .. }
            | ChatMessageContentPartValue::Video { .. } => attachments += 1,
        }
    }
    (text_parts.join(" "), attachments)
}

fn chat_message_content_source_to_owned(
    source: &ChatMessageContentSourceValue<'_>,
) -> ChatMessageContentSource {
    match source {
        ChatMessageContentSourceValue::Url { url } => ChatMessageContentSource::Url {
            url: Cow::Owned(url.to_string()),
        },
        ChatMessageContentSourceValue::Bytes { data, media_type } => {
            ChatMessageContentSource::Bytes {
                data: Cow::Owned(data.to_vec()),
                media_type: media_type
                    .as_deref()
                    .map(|media_type| Cow::Owned(media_type.to_string())),
            }
        }
        ChatMessageContentSourceValue::S3 { uri, bucket_owner } => ChatMessageContentSource::S3 {
            uri: Cow::Owned(uri.to_string()),
            bucket_owner: bucket_owner
                .as_deref()
                .map(|bucket_owner| Cow::Owned(bucket_owner.to_string())),
        },
        ChatMessageContentSourceValue::FileId { file_id } => ChatMessageContentSource::FileId {
            file_id: Cow::Owned(file_id.to_string()),
        },
    }
}

fn chat_message_content_part_to_owned(
    part: &ChatMessageContentPartValue<'_>,
) -> ChatMessageContentPart {
    match part {
        ChatMessageContentPartValue::Text { text } => ChatMessageContentPart::Text {
            text: Cow::Owned(text.to_string()),
        },
        ChatMessageContentPartValue::Image { source, format } => ChatMessageContentPart::Image {
            source: chat_message_content_source_to_owned(source),
            format: format
                .as_deref()
                .map(|format| Cow::Owned(format.to_string())),
        },
        ChatMessageContentPartValue::Document {
            source,
            format,
            name,
        } => ChatMessageContentPart::Document {
            source: chat_message_content_source_to_owned(source),
            format: format
                .as_deref()
                .map(|format| Cow::Owned(format.to_string())),
            name: name.as_deref().map(|name| Cow::Owned(name.to_string())),
        },
        ChatMessageContentPartValue::Audio { source, format } => ChatMessageContentPart::Audio {
            source: chat_message_content_source_to_owned(source),
            format: format
                .as_deref()
                .map(|format| Cow::Owned(format.to_string())),
        },
        ChatMessageContentPartValue::Video { source, format } => ChatMessageContentPart::Video {
            source: chat_message_content_source_to_owned(source),
            format: format
                .as_deref()
                .map(|format| Cow::Owned(format.to_string())),
        },
    }
}

fn truncate_data_url(url: &str) -> Cow<'_, str> {
    const MAX_DATA_PREVIEW: usize = 32;

    if !url.starts_with("data:") {
        return Cow::Borrowed(url);
    }

    let Some((prefix, data)) = url.split_once(',') else {
        return Cow::Borrowed(url);
    };

    if data.len() <= MAX_DATA_PREVIEW {
        return Cow::Borrowed(url);
    }

    let preview = &data[..MAX_DATA_PREVIEW];
    let truncated = data.len() - MAX_DATA_PREVIEW;

    Cow::Owned(format!(
        "{prefix},{preview}...[truncated {truncated} chars]"
    ))
}
