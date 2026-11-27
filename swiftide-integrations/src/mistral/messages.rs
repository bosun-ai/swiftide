//! Encode common messages using unchanged async-openai request types.
use anyhow::Result;
use async_openai::types::chat::{
    ChatCompletionMessageToolCall, ChatCompletionMessageToolCalls,
    ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessageContentPartAudio,
    ChatCompletionRequestMessageContentPartImage, ChatCompletionRequestMessageContentPartText,
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestToolMessageArgs,
    ChatCompletionRequestUserMessageArgs, ChatCompletionRequestUserMessageContent,
    ChatCompletionRequestUserMessageContentPart, FunctionCall, ImageUrl, InputAudio,
    InputAudioFormat,
};
use base64::Engine as _;
use swiftide_core::chat_completion::{
    ChatMessage, ChatMessageContentPart, ChatMessageContentSource,
};

pub(super) fn message_to_openai(
    message: &ChatMessage,
) -> Result<Option<async_openai::types::chat::ChatCompletionRequestMessage>> {
    let openai_message = match message {
        ChatMessage::User(msg) => ChatCompletionRequestUserMessageArgs::default()
            .content(msg.as_ref())
            .build()?
            .into(),
        ChatMessage::UserWithParts(parts) => ChatCompletionRequestUserMessageArgs::default()
            .content(user_parts_to_openai(parts)?)
            .build()?
            .into(),
        ChatMessage::System(msg) => ChatCompletionRequestSystemMessageArgs::default()
            .content(msg.as_ref())
            .build()?
            .into(),
        ChatMessage::Summary(msg) => ChatCompletionRequestAssistantMessageArgs::default()
            .content(msg.as_ref())
            .build()?
            .into(),
        ChatMessage::ToolOutput(tool_call, tool_output) => {
            let Some(content) = tool_output.content() else {
                return Ok(Some(
                    ChatCompletionRequestToolMessageArgs::default()
                        .tool_call_id(tool_call.id())
                        .build()?
                        .into(),
                ));
            };

            ChatCompletionRequestToolMessageArgs::default()
                .content(content)
                .tool_call_id(tool_call.id())
                .build()?
                .into()
        }
        ChatMessage::Assistant(content, tool_calls) => {
            let mut builder = ChatCompletionRequestAssistantMessageArgs::default();

            let has_tool_calls = tool_calls.as_ref().is_some_and(|calls| !calls.is_empty());

            if let Some(content) = content.as_deref() {
                builder.content(content);
            }

            if let Some(tool_calls) = tool_calls.as_ref() {
                let calls = tool_calls
                    .iter()
                    .map(|tool_call| {
                        ChatCompletionMessageToolCalls::Function(ChatCompletionMessageToolCall {
                            id: tool_call.id().to_string(),
                            function: FunctionCall {
                                name: tool_call.name().to_string(),
                                arguments: tool_call.args().unwrap_or_default().to_string(),
                            },
                        })
                    })
                    .collect::<Vec<_>>();

                builder.tool_calls(calls);
            }

            if content.is_none() && !has_tool_calls {
                return Ok(None);
            }

            builder.build()?.into()
        }
        ChatMessage::Reasoning(_) => return Ok(None),
    };

    Ok(Some(openai_message))
}

fn user_parts_to_openai(
    parts: &[ChatMessageContentPart],
) -> Result<ChatCompletionRequestUserMessageContent> {
    let mapped = parts
        .iter()
        .map(part_to_openai_user_content_part)
        .collect::<Result<Vec<_>>>()?;

    Ok(ChatCompletionRequestUserMessageContent::Array(mapped))
}

fn part_to_openai_user_content_part(
    part: &ChatMessageContentPart,
) -> Result<ChatCompletionRequestUserMessageContentPart> {
    Ok(match part {
        ChatMessageContentPart::Text { text } => ChatCompletionRequestUserMessageContentPart::from(
            ChatCompletionRequestMessageContentPartText::from(text.as_ref()),
        ),
        ChatMessageContentPart::Image { source, .. } => {
            let image_url = ImageUrl {
                url: source_to_openai_url(source)?,
                detail: None,
            };
            ChatCompletionRequestUserMessageContentPart::from(
                ChatCompletionRequestMessageContentPartImage { image_url },
            )
        }
        ChatMessageContentPart::Audio { source, format } => {
            let ChatMessageContentSource::Bytes { data, .. } = source else {
                anyhow::bail!("OpenAI chat input_audio only supports bytes sources");
            };

            let format = match format.as_deref() {
                Some("wav") => InputAudioFormat::Wav,
                Some("mp3") | None => InputAudioFormat::Mp3,
                Some(other) => anyhow::bail!("Unsupported OpenAI chat input_audio format: {other}"),
            };

            let input_audio = InputAudio {
                data: base64::engine::general_purpose::STANDARD.encode(data),
                format,
            };

            ChatCompletionRequestUserMessageContentPart::from(
                ChatCompletionRequestMessageContentPartAudio { input_audio },
            )
        }
        ChatMessageContentPart::Document { .. } => {
            anyhow::bail!("OpenAI chat file parts are not supported by async-openai yet")
        }
        ChatMessageContentPart::Video { .. } => {
            anyhow::bail!("OpenAI chat completion does not support video parts")
        }
    })
}

fn source_to_openai_url(source: &ChatMessageContentSource) -> Result<String> {
    match source {
        ChatMessageContentSource::Url { url } => Ok(url.clone()),
        ChatMessageContentSource::FileId { .. } => {
            anyhow::bail!("OpenAI chat image_url does not accept file_id sources")
        }
        ChatMessageContentSource::S3 { .. } => {
            anyhow::bail!("OpenAI chat image_url does not accept s3 sources")
        }
        ChatMessageContentSource::Bytes { data, media_type } => {
            let media_type = media_type.as_deref().unwrap_or("application/octet-stream");
            let encoded = base64::engine::general_purpose::STANDARD.encode(data);
            Ok(format!("data:{media_type};base64,{encoded}"))
        }
    }
}
