use std::collections::{HashMap, HashSet};

use anyhow::Context as _;
use async_trait::async_trait;
use aws_sdk_bedrockruntime::{
    operation::converse::ConverseOutput,
    types::{
        AudioBlock, AudioFormat, AudioSource, AutoToolChoice, ContentBlock, ContentBlockDelta,
        ContentBlockStart, ConversationRole, ConverseOutput as ConverseResult,
        ConverseStreamOutput, DocumentBlock, DocumentFormat, DocumentSource, ImageBlock,
        ImageFormat, ImageSource, InferenceConfiguration, Message, ReasoningContentBlock,
        ReasoningContentBlockDelta, ReasoningTextBlock, S3Location, StopReason, SystemContentBlock,
        Tool, ToolChoice, ToolConfiguration, ToolInputSchema, ToolResultBlock,
        ToolResultContentBlock, ToolResultStatus, ToolSpecification, ToolUseBlock, VideoBlock,
        VideoFormat, VideoSource,
    },
};
use aws_smithy_json::{
    deserialize::{json_token_iter, token::expect_document},
    serialize::JsonValueWriter,
};
use aws_smithy_types::{Blob, Document};
use base64::Engine as _;
use futures_util::stream;
use swiftide_core::{
    ChatCompletion, ChatCompletionStream,
    chat_completion::{
        ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ChatMessageContentPart,
        ChatMessageContentSource, ReasoningItem, ToolCall, ToolOutput, ToolSpec,
        errors::LanguageModelError,
    },
};

#[cfg(feature = "metrics")]
use swiftide_core::metrics::emit_usage;

use super::{AwsBedrock, Options};

#[async_trait]
impl ChatCompletion for AwsBedrock {
    #[tracing::instrument(skip_all, err)]
    async fn complete(
        &self,
        request: &ChatCompletionRequest<'_>,
    ) -> Result<ChatCompletionResponse, LanguageModelError> {
        let model = self.prompt_model()?;
        let (messages, system, inference_config, tool_config) =
            build_converse_input(request, &self.default_options)?;

        tracing::debug!(
            model = model,
            inference_config = ?inference_config,
            has_tool_config = tool_config.is_some(),
            "[ChatCompletion] Request to bedrock converse"
        );

        let response = self
            .client
            .converse(
                model,
                messages,
                system,
                inference_config,
                tool_config,
                None,
                self.default_options.additional_model_request_fields.clone(),
                self.default_options
                    .additional_model_response_field_paths
                    .clone(),
            )
            .await?;

        tracing::debug!(response = ?response, "[ChatCompletion] Response from bedrock converse");

        let completion = response_to_chat_completion(&response)?;

        if let Some(error) = super::context_length_exceeded_if_empty(
            completion.message.is_some(),
            completion.tool_calls.is_some(),
            completion
                .reasoning
                .as_ref()
                .is_some_and(|reasoning| !reasoning.is_empty()),
            Some(response.stop_reason()),
        ) {
            return Err(error);
        }

        if let Some(usage) = completion.usage.as_ref() {
            self.report_usage(model, usage).await?;
        }

        Ok(completion)
    }

    #[tracing::instrument(skip_all)]
    async fn complete_stream(&self, request: &ChatCompletionRequest<'_>) -> ChatCompletionStream {
        let model = match self.prompt_model() {
            Ok(model) => model.to_string(),
            Err(error) => return error.into(),
        };

        let (messages, system, inference_config, tool_config) =
            match build_converse_input(request, &self.default_options) {
                Ok(parts) => parts,
                Err(error) => return error.into(),
            };

        let stream_output = match self
            .client
            .converse_stream(
                &model,
                messages,
                system,
                inference_config,
                tool_config,
                self.default_options.additional_model_request_fields.clone(),
                self.default_options
                    .additional_model_response_field_paths
                    .clone(),
            )
            .await
        {
            Ok(stream_output) => stream_output,
            Err(error) => return error.into(),
        };

        let on_usage = self.on_usage.clone();
        #[cfg(feature = "metrics")]
        let metric_metadata = std::sync::Arc::new(self.metric_metadata.clone());

        let event_stream = stream_output.stream;
        let stream = stream::unfold(
            (
                event_stream,
                ChatCompletionResponse::default(),
                None::<StopReason>,
                false,
                model,
            ),
            move |(mut event_stream, mut response, mut stop_reason, finished, model)| {
                let on_usage = on_usage.clone();
                #[cfg(feature = "metrics")]
                let metric_metadata = metric_metadata.clone();

                async move {
                    if finished {
                        return None;
                    }

                    match event_stream.recv().await {
                        Ok(Some(event)) => {
                            apply_stream_event(&event, &mut response, &mut stop_reason);
                            Some((
                                Ok(response.clone()),
                                (event_stream, response, stop_reason, false, model),
                            ))
                        }
                        Ok(None) => {
                            if let Some(error) = super::context_length_exceeded_if_empty(
                                response.message.is_some(),
                                response.tool_calls.is_some(),
                                response
                                    .reasoning
                                    .as_ref()
                                    .is_some_and(|reasoning| !reasoning.is_empty()),
                                stop_reason.as_ref(),
                            ) {
                                return Some((
                                    Err(error),
                                    (event_stream, response, stop_reason, true, model),
                                ));
                            }

                            if let Some(usage) = response.usage.as_ref() {
                                if let Some(callback) = on_usage.as_ref()
                                    && let Err(error) = callback(usage).await
                                {
                                    return Some((
                                        Err(LanguageModelError::permanent(error)),
                                        (event_stream, response, stop_reason, true, model),
                                    ));
                                }

                                #[cfg(feature = "metrics")]
                                emit_usage(
                                    &model,
                                    usage.prompt_tokens.into(),
                                    usage.completion_tokens.into(),
                                    usage.total_tokens.into(),
                                    metric_metadata.as_ref().as_ref(),
                                );
                            }

                            Some((
                                Ok(response.clone()),
                                (event_stream, response, stop_reason, true, model),
                            ))
                        }
                        Err(error) => Some((
                            Err(super::converse_stream_output_error_to_language_model_error(
                                error,
                            )),
                            (event_stream, response, stop_reason, true, model),
                        )),
                    }
                }
            },
        );

        Box::pin(stream)
    }
}

fn build_converse_input(
    request: &ChatCompletionRequest<'_>,
    options: &Options,
) -> Result<
    (
        Vec<Message>,
        Option<Vec<SystemContentBlock>>,
        Option<InferenceConfiguration>,
        Option<ToolConfiguration>,
    ),
    LanguageModelError,
> {
    let source_messages = request.messages();
    let mut messages = Vec::with_capacity(source_messages.len());
    let mut system = Vec::new();

    for message in source_messages {
        match message {
            ChatMessage::System(text) => {
                system.push(SystemContentBlock::Text(text.as_ref().to_owned()));
            }
            ChatMessage::Summary(text) => {
                messages.push(user_message_from_text(text.as_ref().to_owned())?);
            }
            ChatMessage::User(text) => {
                messages.push(user_message_from_text(text.as_ref().to_owned())?);
            }
            ChatMessage::UserWithParts(parts) => messages.push(user_message_from_parts(parts)?),
            ChatMessage::Assistant(content, maybe_tool_calls) => {
                let mut blocks = Vec::with_capacity(
                    usize::from(content.as_ref().is_some_and(|text| !text.is_empty()))
                        + maybe_tool_calls.as_ref().map_or(0, Vec::len),
                );

                if let Some(content) = content.as_ref()
                    && !content.is_empty()
                {
                    blocks.push(ContentBlock::Text(content.as_ref().to_owned()));
                }

                if let Some(tool_calls) = maybe_tool_calls.as_ref() {
                    for tool_call in tool_calls {
                        let input =
                            tool_call_args_to_document(tool_call.args()).with_context(|| {
                                format!("Invalid JSON args for tool call {}", tool_call.name())
                            })?;
                        let tool_use = ToolUseBlock::builder()
                            .tool_use_id(tool_call.id())
                            .name(tool_call.name())
                            .input(input)
                            .build()
                            .map_err(LanguageModelError::permanent)?;
                        blocks.push(ContentBlock::ToolUse(tool_use));
                    }
                }

                if !blocks.is_empty() {
                    messages.push(message_from_blocks(ConversationRole::Assistant, blocks)?);
                }
            }
            ChatMessage::ToolOutput(tool_call, output) => {
                let status = match output {
                    ToolOutput::Fail(_) => Some(ToolResultStatus::Error),
                    _ => Some(ToolResultStatus::Success),
                };

                let tool_result = ToolResultBlock::builder()
                    .tool_use_id(tool_call.id())
                    .content(tool_output_to_content_block(output)?)
                    .set_status(status)
                    .build()
                    .map_err(LanguageModelError::permanent)?;

                messages.push(message_from_blocks(
                    ConversationRole::User,
                    vec![ContentBlock::ToolResult(tool_result)],
                )?);
            }
            ChatMessage::Reasoning(item) => {
                if let Some(reasoning_message) = assistant_reasoning_message_from_item(item)? {
                    messages.push(reasoning_message);
                }
            }
        }
    }

    if messages.is_empty() {
        return Err(LanguageModelError::permanent(
            "Bedrock Converse requires at least one non-system message",
        ));
    }

    Ok((
        messages,
        (!system.is_empty()).then_some(system),
        super::inference_config_from_options(options),
        tool_config_from_specs(request.tools_spec(), options.tool_strict_enabled())?,
    ))
}

fn user_message_from_text(text: String) -> Result<Message, LanguageModelError> {
    message_from_blocks(ConversationRole::User, vec![ContentBlock::Text(text)])
}

fn user_message_from_parts(
    parts: &[ChatMessageContentPart],
) -> Result<Message, LanguageModelError> {
    let mut blocks = Vec::with_capacity(parts.len());
    let mut has_text = false;
    let mut has_document = false;

    for part in parts {
        match part {
            ChatMessageContentPart::Text { text } => {
                if !text.is_empty() {
                    blocks.push(ContentBlock::Text(text.as_ref().to_owned()));
                    has_text = true;
                }
            }
            ChatMessageContentPart::Image { source, format } => {
                blocks.push(ContentBlock::Image(image_block_from_part(
                    source,
                    format.as_deref(),
                )?));
            }
            ChatMessageContentPart::Document {
                source,
                format,
                name,
            } => {
                blocks.push(ContentBlock::Document(document_block_from_part(
                    source,
                    format.as_deref(),
                    name.as_deref(),
                )?));
                has_document = true;
            }
            ChatMessageContentPart::Audio { source, format } => {
                blocks.push(ContentBlock::Audio(audio_block_from_part(
                    source,
                    format.as_deref(),
                )?));
            }
            ChatMessageContentPart::Video { source, format } => {
                blocks.push(ContentBlock::Video(video_block_from_part(
                    source,
                    format.as_deref(),
                )?));
            }
        }
    }

    if blocks.is_empty() {
        return Err(LanguageModelError::permanent(
            "UserWithParts requires at least one content part",
        ));
    }

    if has_document && !has_text {
        return Err(LanguageModelError::permanent(
            "Bedrock document parts require at least one text part in the same message",
        ));
    }

    message_from_blocks(ConversationRole::User, blocks)
}

fn image_block_from_part(
    source: &ChatMessageContentSource,
    format: Option<&str>,
) -> Result<ImageBlock, LanguageModelError> {
    let format = image_format_from_source(format, source)?;
    let source = image_source_from_content_source(source)?;

    ImageBlock::builder()
        .format(format)
        .source(source)
        .build()
        .map_err(LanguageModelError::permanent)
}

fn document_block_from_part(
    source: &ChatMessageContentSource,
    format: Option<&str>,
    name: Option<&str>,
) -> Result<DocumentBlock, LanguageModelError> {
    let format = document_format_from_source(format, source)?;
    let source = document_source_from_content_source(source)?;
    let name = name.unwrap_or("document");

    DocumentBlock::builder()
        .format(format)
        .name(name)
        .source(source)
        .build()
        .map_err(LanguageModelError::permanent)
}

fn audio_block_from_part(
    source: &ChatMessageContentSource,
    format: Option<&str>,
) -> Result<AudioBlock, LanguageModelError> {
    let format = audio_format_from_source(format, source)?;
    let source = audio_source_from_content_source(source)?;

    AudioBlock::builder()
        .format(format)
        .source(source)
        .build()
        .map_err(LanguageModelError::permanent)
}

fn video_block_from_part(
    source: &ChatMessageContentSource,
    format: Option<&str>,
) -> Result<VideoBlock, LanguageModelError> {
    let format = video_format_from_source(format, source)?;
    let source = video_source_from_content_source(source)?;

    VideoBlock::builder()
        .format(format)
        .source(source)
        .build()
        .map_err(LanguageModelError::permanent)
}

fn image_source_from_content_source(
    source: &ChatMessageContentSource,
) -> Result<ImageSource, LanguageModelError> {
    source_from_content_source(source, "image", ImageSource::Bytes, ImageSource::S3Location)
}

fn document_source_from_content_source(
    source: &ChatMessageContentSource,
) -> Result<DocumentSource, LanguageModelError> {
    source_from_content_source(
        source,
        "document",
        DocumentSource::Bytes,
        DocumentSource::S3Location,
    )
}

fn audio_source_from_content_source(
    source: &ChatMessageContentSource,
) -> Result<AudioSource, LanguageModelError> {
    source_from_content_source(source, "audio", AudioSource::Bytes, AudioSource::S3Location)
}

fn video_source_from_content_source(
    source: &ChatMessageContentSource,
) -> Result<VideoSource, LanguageModelError> {
    source_from_content_source(source, "video", VideoSource::Bytes, VideoSource::S3Location)
}

fn source_from_content_source<T>(
    source: &ChatMessageContentSource,
    label: &str,
    from_bytes: impl Fn(Blob) -> T,
    from_s3: impl Fn(S3Location) -> T,
) -> Result<T, LanguageModelError> {
    match source {
        ChatMessageContentSource::Bytes { data, .. } => Ok(from_bytes(Blob::new(data.to_vec()))),
        ChatMessageContentSource::S3 { uri, bucket_owner } => {
            Ok(from_s3(s3_location(uri, bucket_owner.as_deref())?))
        }
        ChatMessageContentSource::Url { url } => {
            if is_s3_url(url) {
                Ok(from_s3(s3_location(url, None)?))
            } else if let Some((_, encoded)) = parse_data_url(url) {
                Ok(from_bytes(Blob::new(decode_data_url_bytes(encoded)?)))
            } else {
                Err(LanguageModelError::permanent(format!(
                    "Bedrock {label} source URL must be data: or s3://"
                )))
            }
        }
        ChatMessageContentSource::FileId { .. } => Err(LanguageModelError::permanent(format!(
            "Bedrock does not support file_id {label} sources"
        ))),
    }
}

fn image_format_from_source(
    format: Option<&str>,
    source: &ChatMessageContentSource,
) -> Result<ImageFormat, LanguageModelError> {
    resolve_format(
        format,
        source,
        infer_image_format_from_source,
        |value| ImageFormat::try_parse(value).ok(),
        "image",
    )
}

fn document_format_from_source(
    format: Option<&str>,
    source: &ChatMessageContentSource,
) -> Result<DocumentFormat, LanguageModelError> {
    resolve_format(
        format,
        source,
        infer_document_format_from_source,
        |value| DocumentFormat::try_parse(value).ok(),
        "document",
    )
}

fn audio_format_from_source(
    format: Option<&str>,
    source: &ChatMessageContentSource,
) -> Result<AudioFormat, LanguageModelError> {
    resolve_format(
        format,
        source,
        infer_audio_format_from_source,
        |value| AudioFormat::try_parse(value).ok(),
        "audio",
    )
}

fn video_format_from_source(
    format: Option<&str>,
    source: &ChatMessageContentSource,
) -> Result<VideoFormat, LanguageModelError> {
    resolve_format(
        format,
        source,
        infer_video_format_from_source,
        |value| VideoFormat::try_parse(value).ok(),
        "video",
    )
}

fn resolve_format<T>(
    explicit_format: Option<&str>,
    source: &ChatMessageContentSource,
    infer: impl Fn(&ChatMessageContentSource) -> Option<&'static str>,
    parse: impl Fn(&str) -> Option<T>,
    label: &str,
) -> Result<T, LanguageModelError> {
    let value = explicit_format.or_else(|| infer(source)).ok_or_else(|| {
        LanguageModelError::permanent(format!("Bedrock {label} format is required"))
    })?;

    parse(value).ok_or_else(|| {
        LanguageModelError::permanent(format!("Unsupported Bedrock {label} format: {value}"))
    })
}

fn infer_image_format_from_source(source: &ChatMessageContentSource) -> Option<&'static str> {
    infer_format_from_source(
        source,
        IMAGE_MEDIA_TYPE_FORMATS,
        IMAGE_EXTENSION_FORMATS,
        None,
    )
}

fn infer_document_format_from_source(source: &ChatMessageContentSource) -> Option<&'static str> {
    infer_format_from_source(
        source,
        DOCUMENT_MEDIA_TYPE_FORMATS,
        DOCUMENT_EXTENSION_FORMATS,
        Some("txt"),
    )
}

fn infer_audio_format_from_source(source: &ChatMessageContentSource) -> Option<&'static str> {
    infer_format_from_source(
        source,
        AUDIO_MEDIA_TYPE_FORMATS,
        AUDIO_EXTENSION_FORMATS,
        None,
    )
}

fn infer_video_format_from_source(source: &ChatMessageContentSource) -> Option<&'static str> {
    infer_format_from_source(
        source,
        VIDEO_MEDIA_TYPE_FORMATS,
        VIDEO_EXTENSION_FORMATS,
        None,
    )
}

fn infer_format_from_source(
    source: &ChatMessageContentSource,
    media_type_mappings: &[(&'static str, &'static str)],
    extension_mappings: &[(&'static str, &'static str)],
    fallback: Option<&'static str>,
) -> Option<&'static str> {
    match source {
        ChatMessageContentSource::Bytes { media_type, .. } => media_type
            .as_deref()
            .and_then(|media_type| mapped_format(media_type, media_type_mappings))
            .or(fallback),
        ChatMessageContentSource::Url { url } => if let Some((media_type, _)) = parse_data_url(url)
        {
            mapped_format(media_type, media_type_mappings)
        } else {
            extension_from_url(url)
                .and_then(|extension| mapped_format(extension, extension_mappings))
        }
        .or(fallback),
        ChatMessageContentSource::S3 { uri, .. } => extension_from_url(uri)
            .and_then(|extension| mapped_format(extension, extension_mappings))
            .or(fallback),
        ChatMessageContentSource::FileId { .. } => fallback,
    }
}

fn s3_location(uri: &str, bucket_owner: Option<&str>) -> Result<S3Location, LanguageModelError> {
    let mut builder = S3Location::builder().uri(uri);
    if let Some(bucket_owner) = bucket_owner {
        builder = builder.bucket_owner(bucket_owner);
    }

    builder.build().map_err(LanguageModelError::permanent)
}

fn is_s3_url(url: &str) -> bool {
    url.starts_with("s3://")
}

fn parse_data_url(url: &str) -> Option<(&str, &str)> {
    let rest = url.strip_prefix("data:")?;
    let (header, data) = rest.split_once(',')?;
    let media_type = header.strip_suffix(";base64")?;
    Some((media_type, data))
}

fn decode_data_url_bytes(encoded: &str) -> Result<Vec<u8>, LanguageModelError> {
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(LanguageModelError::permanent)
}

fn extension_from_url(url: &str) -> Option<&str> {
    let without_query = url.split(['?', '#']).next()?;
    let filename = without_query.rsplit('/').next()?;
    let (_, extension) = filename.rsplit_once('.')?;
    Some(extension)
}

fn mapped_format(value: &str, mappings: &[(&'static str, &'static str)]) -> Option<&'static str> {
    mappings
        .iter()
        .find_map(|(input, output)| input.eq_ignore_ascii_case(value).then_some(*output))
}

fn message_from_blocks(
    role: ConversationRole,
    blocks: Vec<ContentBlock>,
) -> Result<Message, LanguageModelError> {
    Message::builder()
        .role(role)
        .set_content(Some(blocks))
        .build()
        .map_err(LanguageModelError::permanent)
}

fn tool_output_to_content_block(
    output: &ToolOutput,
) -> Result<ToolResultContentBlock, LanguageModelError> {
    match output {
        ToolOutput::Text(text) | ToolOutput::Fail(text) => {
            Ok(ToolResultContentBlock::Text(text.clone()))
        }
        ToolOutput::FeedbackRequired(Some(value))
        | ToolOutput::Stop(Some(value))
        | ToolOutput::AgentFailed(Some(value)) => {
            Ok(ToolResultContentBlock::Json(json_value_to_document(value)?))
        }
        _ => Ok(ToolResultContentBlock::Text(output.to_string())),
    }
}

fn tool_call_args_to_document(args: Option<&str>) -> Result<Document, LanguageModelError> {
    match args.map(str::trim) {
        Some(args) if !args.is_empty() => parse_document_json_bytes(args.as_bytes())
            .with_context(|| format!("Failed to parse tool args as JSON: {args}"))
            .map_err(LanguageModelError::permanent),
        _ => Ok(Document::Object(HashMap::new())),
    }
}

fn tool_config_from_specs(
    tool_specs: &HashSet<ToolSpec>,
    strict: bool,
) -> Result<Option<ToolConfiguration>, LanguageModelError> {
    if tool_specs.is_empty() {
        return Ok(None);
    }

    let tools = tool_specs
        .iter()
        .map(|spec| tool_spec_to_bedrock(spec, strict))
        .collect::<Result<Vec<_>, _>>()?;

    let tool_config = ToolConfiguration::builder()
        .set_tools(Some(tools))
        .tool_choice(ToolChoice::Auto(AutoToolChoice::builder().build()))
        .build()
        .map_err(LanguageModelError::permanent)?;

    Ok(Some(tool_config))
}

fn tool_spec_to_bedrock(spec: &ToolSpec, strict: bool) -> Result<Tool, LanguageModelError> {
    let input_schema = match spec.parameters_schema.as_ref() {
        Some(schema) => {
            let schema_value =
                serde_json::to_value(schema).map_err(LanguageModelError::permanent)?;
            ToolInputSchema::Json(json_value_to_document(&schema_value)?)
        }
        None => ToolInputSchema::Json(Document::Object(HashMap::new())),
    };

    let mut builder = ToolSpecification::builder()
        .name(spec.name.clone())
        .input_schema(input_schema)
        .strict(strict);

    if !spec.description.is_empty() {
        builder = builder.description(spec.description.clone());
    }

    let tool_spec = builder.build().map_err(LanguageModelError::permanent)?;
    Ok(Tool::ToolSpec(tool_spec))
}

pub(super) fn response_to_chat_completion(
    response: &ConverseOutput,
) -> Result<ChatCompletionResponse, LanguageModelError> {
    let (message, tool_calls, reasoning) = match response.output() {
        Some(output) => match output {
            ConverseResult::Message(message) => extract_message_and_tool_calls(message)?,
            _ => (None, None, Vec::new()),
        },
        None => (None, None, Vec::new()),
    };

    let mut builder = ChatCompletionResponse::builder()
        .maybe_message(message)
        .maybe_tool_calls(tool_calls)
        .to_owned();

    if !reasoning.is_empty() {
        builder.reasoning(reasoning);
    }

    if let Some(usage) = response.usage() {
        builder.usage(super::usage_from_bedrock(usage));
    }

    builder.build().map_err(LanguageModelError::from)
}

fn extract_message_and_tool_calls(
    message: &Message,
) -> Result<(Option<String>, Option<Vec<ToolCall>>, Vec<ReasoningItem>), LanguageModelError> {
    let mut text = String::new();
    let mut has_text = false;
    let mut tool_calls = Vec::with_capacity(message.content().len());
    let mut reasoning = Vec::new();

    for (content_block_index, block) in message.content().iter().enumerate() {
        match block {
            ContentBlock::Text(block_text) => {
                text.push_str(block_text);
                has_text = true;
            }
            ContentBlock::ToolUse(tool_use) => {
                let args = document_to_json_string(tool_use.input())?;
                let tool_call = ToolCall::builder()
                    .id(tool_use.tool_use_id())
                    .name(tool_use.name())
                    .args(args)
                    .build()
                    .map_err(LanguageModelError::permanent)?;
                tool_calls.push(tool_call);
            }
            ContentBlock::ReasoningContent(ReasoningContentBlock::ReasoningText(
                reasoning_text,
            )) => {
                reasoning.push(reasoning_item_from_reasoning_text(
                    content_block_index,
                    reasoning_text.text(),
                    reasoning_text.signature(),
                ));
            }
            _ => {}
        }
    }

    let message = has_text.then_some(text);
    let tool_calls = (!tool_calls.is_empty()).then_some(tool_calls);

    Ok((message, tool_calls, reasoning))
}

fn document_to_json_string(document: &Document) -> Result<String, LanguageModelError> {
    let mut output = String::new();
    JsonValueWriter::new(&mut output).document(document);
    Ok(output)
}

fn apply_stream_event(
    event: &ConverseStreamOutput,
    response: &mut ChatCompletionResponse,
    stop_reason: &mut Option<StopReason>,
) {
    match event {
        ConverseStreamOutput::ContentBlockStart(event) => {
            if let (Some(ContentBlockStart::ToolUse(tool_use)), Ok(index)) =
                (event.start(), usize::try_from(event.content_block_index()))
            {
                response.append_tool_call_delta(
                    index,
                    Some(tool_use.tool_use_id()),
                    Some(tool_use.name()),
                    None,
                );
            }
        }
        ConverseStreamOutput::ContentBlockDelta(event) => {
            let Ok(index) = usize::try_from(event.content_block_index()) else {
                return;
            };

            let Some(delta) = event.delta() else {
                return;
            };

            match delta {
                ContentBlockDelta::Text(text) => {
                    response.append_message_delta(Some(text));
                }
                ContentBlockDelta::ToolUse(delta) => {
                    response.append_tool_call_delta(index, None, None, Some(delta.input()));
                }
                ContentBlockDelta::ReasoningContent(delta) => {
                    apply_reasoning_delta(response, index, delta);
                }
                _ => {}
            }
        }
        ConverseStreamOutput::MessageStop(event) => {
            *stop_reason = Some(event.stop_reason().clone());
        }
        ConverseStreamOutput::Metadata(event) => {
            if let Some(usage) = event.usage() {
                response.usage = Some(super::usage_from_bedrock(usage));
            }
        }
        _ => {}
    }
}

fn assistant_reasoning_message_from_item(
    item: &ReasoningItem,
) -> Result<Option<Message>, LanguageModelError> {
    let text = item
        .content
        .as_ref()
        .and_then(|content| content.first())
        .map(String::as_str)
        .filter(|text| !text.is_empty());
    let signature = item
        .encrypted_content
        .as_deref()
        .filter(|value| !value.is_empty());

    let (Some(text), Some(signature)) = (text, signature) else {
        return Ok(None);
    };

    let reasoning_text_block = ReasoningTextBlock::builder()
        .text(text)
        .signature(signature)
        .build()
        .map_err(LanguageModelError::permanent)?;

    message_from_blocks(
        ConversationRole::Assistant,
        vec![ContentBlock::ReasoningContent(
            ReasoningContentBlock::ReasoningText(reasoning_text_block),
        )],
    )
    .map(Some)
}

fn reasoning_item_from_reasoning_text(
    content_block_index: usize,
    text: &str,
    signature: Option<&str>,
) -> ReasoningItem {
    ReasoningItem {
        id: format!("bedrock_reasoning_{content_block_index}"),
        summary: Vec::new(),
        content: Some(vec![text.to_string()]),
        encrypted_content: signature.map(ToString::to_string),
        status: None,
    }
}

fn apply_reasoning_delta(
    response: &mut ChatCompletionResponse,
    content_block_index: usize,
    delta: &ReasoningContentBlockDelta,
) {
    let reasoning_item = ensure_reasoning_item(response, content_block_index);

    match delta {
        ReasoningContentBlockDelta::Text(text) => {
            let content = reasoning_item
                .content
                .get_or_insert_with(|| vec![String::new()]);
            if content.is_empty() {
                content.push(String::new());
            }
            content[0].push_str(text);
        }
        ReasoningContentBlockDelta::Signature(signature) => {
            reasoning_item.encrypted_content = Some(signature.clone());
        }
        _ => {}
    }
}

fn ensure_reasoning_item(
    response: &mut ChatCompletionResponse,
    content_block_index: usize,
) -> &mut ReasoningItem {
    let reasoning = response.reasoning.get_or_insert_with(Vec::new);
    let reasoning_id = format!("bedrock_reasoning_{content_block_index}");
    if let Some(position) = reasoning.iter().position(|item| item.id == reasoning_id) {
        return reasoning
            .get_mut(position)
            .expect("position from iter().position must exist");
    }

    reasoning.push(ReasoningItem {
        id: reasoning_id,
        summary: Vec::new(),
        content: None,
        encrypted_content: None,
        status: None,
    });

    reasoning
        .last_mut()
        .expect("pushed reasoning item must exist")
}

fn json_value_to_document(value: &serde_json::Value) -> Result<Document, LanguageModelError> {
    let bytes = serde_json::to_vec(value).map_err(LanguageModelError::permanent)?;
    parse_document_json_bytes(&bytes).map_err(LanguageModelError::permanent)
}

fn parse_document_json_bytes(input: &[u8]) -> anyhow::Result<Document> {
    let mut tokens = json_token_iter(input).peekable();
    let document = expect_document(&mut tokens)?;

    if tokens.next().transpose()?.is_some() {
        anyhow::bail!("JSON input must contain exactly one value");
    }

    Ok(document)
}

const IMAGE_MEDIA_TYPE_FORMATS: &[(&str, &str)] = &[
    ("image/gif", "gif"),
    ("image/jpeg", "jpeg"),
    ("image/jpg", "jpeg"),
    ("image/png", "png"),
    ("image/webp", "webp"),
];

const IMAGE_EXTENSION_FORMATS: &[(&str, &str)] = &[
    ("gif", "gif"),
    ("jpeg", "jpeg"),
    ("jpg", "jpeg"),
    ("png", "png"),
    ("webp", "webp"),
];

const DOCUMENT_MEDIA_TYPE_FORMATS: &[(&str, &str)] = &[
    ("text/csv", "csv"),
    ("application/msword", "doc"),
    (
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "docx",
    ),
    ("text/html", "html"),
    ("text/markdown", "md"),
    ("text/x-markdown", "md"),
    ("application/pdf", "pdf"),
    ("text/plain", "txt"),
    ("application/vnd.ms-excel", "xls"),
    (
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "xlsx",
    ),
];

const DOCUMENT_EXTENSION_FORMATS: &[(&str, &str)] = &[
    ("csv", "csv"),
    ("doc", "doc"),
    ("docx", "docx"),
    ("html", "html"),
    ("htm", "html"),
    ("md", "md"),
    ("markdown", "md"),
    ("pdf", "pdf"),
    ("txt", "txt"),
    ("xls", "xls"),
    ("xlsx", "xlsx"),
];

const AUDIO_MEDIA_TYPE_FORMATS: &[(&str, &str)] = &[
    ("audio/aac", "aac"),
    ("audio/flac", "flac"),
    ("audio/m4a", "m4a"),
    ("audio/mka", "mka"),
    ("audio/x-matroska", "mkv"),
    ("audio/mpeg", "mp3"),
    ("audio/mp3", "mp3"),
    ("audio/mp4", "mp4"),
    ("audio/ogg", "ogg"),
    ("audio/opus", "opus"),
    ("audio/wav", "wav"),
    ("audio/x-wav", "wav"),
    ("audio/wave", "wav"),
    ("audio/webm", "webm"),
    ("audio/x-aac", "x-aac"),
];

const AUDIO_EXTENSION_FORMATS: &[(&str, &str)] = &[
    ("aac", "aac"),
    ("flac", "flac"),
    ("m4a", "m4a"),
    ("mka", "mka"),
    ("mkv", "mkv"),
    ("mp3", "mp3"),
    ("mp4", "mp4"),
    ("mpeg", "mpeg"),
    ("mpga", "mpga"),
    ("ogg", "ogg"),
    ("opus", "opus"),
    ("pcm", "pcm"),
    ("wav", "wav"),
    ("webm", "webm"),
    ("x-aac", "x-aac"),
];

const VIDEO_MEDIA_TYPE_FORMATS: &[(&str, &str)] = &[
    ("video/x-flv", "flv"),
    ("video/x-matroska", "mkv"),
    ("video/quicktime", "mov"),
    ("video/mp4", "mp4"),
    ("video/mpeg", "mpeg"),
    ("video/3gpp", "three_gp"),
    ("video/webm", "webm"),
    ("video/x-ms-wmv", "wmv"),
];

const VIDEO_EXTENSION_FORMATS: &[(&str, &str)] = &[
    ("flv", "flv"),
    ("mkv", "mkv"),
    ("mov", "mov"),
    ("mp4", "mp4"),
    ("mpeg", "mpeg"),
    ("mpg", "mpg"),
    ("3gp", "three_gp"),
    ("webm", "webm"),
    ("wmv", "wmv"),
];

#[cfg(test)]
mod tests {
    use aws_sdk_bedrockruntime::{
        operation::converse::ConverseOutput,
        types::{
            ContentBlockDeltaEvent, ContentBlockStart, ContentBlockStartEvent,
            ConverseOutput as ConverseResult, Message, MessageStopEvent, ReasoningContentBlock,
            ReasoningContentBlockDelta, ReasoningTextBlock, StopReason, TokenUsage,
            ToolUseBlockDelta, ToolUseBlockStart,
        },
    };
    use schemars::{JsonSchema, schema_for};
    use swiftide_core::chat_completion::{
        ChatMessage, ChatMessageContentPart, ChatMessageContentSource, ReasoningItem, ToolSpec,
    };

    use super::*;
    use crate::aws_bedrock_v2::{AwsBedrock, MockBedrockConverse};

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, JsonSchema)]
    struct WeatherArgs {
        location: String,
    }

    fn response_with_text_and_tool_call() -> ConverseOutput {
        let mut args = HashMap::new();
        args.insert(
            "location".to_string(),
            Document::String("Amsterdam".to_string()),
        );

        ConverseOutput::builder()
            .output(ConverseResult::Message(
                Message::builder()
                    .role(ConversationRole::Assistant)
                    .content(ContentBlock::Text("Working on it".to_string()))
                    .content(ContentBlock::ToolUse(
                        ToolUseBlock::builder()
                            .tool_use_id("call_1")
                            .name("get_weather")
                            .input(Document::Object(args))
                            .build()
                            .unwrap(),
                    ))
                    .build()
                    .unwrap(),
            ))
            .usage(
                TokenUsage::builder()
                    .input_tokens(10)
                    .output_tokens(8)
                    .total_tokens(18)
                    .build()
                    .unwrap(),
            )
            .stop_reason(StopReason::ToolUse)
            .build()
            .unwrap()
    }

    #[test_log::test(tokio::test)]
    async fn test_complete_maps_text_and_tool_calls() {
        let mut bedrock_mock = MockBedrockConverse::new();

        bedrock_mock
            .expect_converse()
            .once()
            .withf(
                |model_id,
                 messages,
                 system,
                 inference_config,
                 tool_config,
                 output_config,
                 _additional_model_request_fields,
                 _additional_model_response_field_paths| {
                    model_id == "anthropic.claude-3-5-sonnet-20241022-v2:0"
                        && messages.len() == 1
                        && system.is_none()
                        && inference_config.is_none()
                        && tool_config.is_none()
                        && output_config.is_none()
                },
            )
            .returning(|_, _, _, _, _, _, _, _| Ok(response_with_text_and_tool_call()));

        let bedrock = AwsBedrock::builder()
            .test_client(bedrock_mock)
            .default_prompt_model("anthropic.claude-3-5-sonnet-20241022-v2:0")
            .build()
            .unwrap();

        let request = ChatCompletionRequest::builder()
            .messages(vec![ChatMessage::User("Check weather".into())])
            .build()
            .unwrap();

        let response = bedrock.complete(&request).await.unwrap();

        assert_eq!(response.message.as_deref(), Some("Working on it"));
        let tool_call = response
            .tool_calls
            .as_ref()
            .and_then(|calls| calls.first())
            .expect("tool call");
        assert_eq!(tool_call.id(), "call_1");
        assert_eq!(tool_call.name(), "get_weather");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(tool_call.args().unwrap()).unwrap(),
            serde_json::json!({"location":"Amsterdam"})
        );
        assert_eq!(response.usage.unwrap().total_tokens, 18);
    }

    #[test_log::test(tokio::test)]
    async fn test_complete_passes_additional_model_fields() {
        let mut bedrock_mock = MockBedrockConverse::new();

        let mut thinking = HashMap::new();
        thinking.insert("type".to_string(), Document::String("enabled".to_string()));
        thinking.insert("budget_tokens".to_string(), Document::from(512_u64));
        let mut request_fields = HashMap::new();
        request_fields.insert("thinking".to_string(), Document::Object(thinking));
        let request_fields = Document::Object(request_fields);

        bedrock_mock
            .expect_converse()
            .once()
            .withf(
                |model_id,
                 _,
                 _,
                 _,
                 _,
                 _,
                 additional_model_request_fields,
                 additional_model_response_field_paths| {
                    model_id == "anthropic.claude-3-5-sonnet-20241022-v2:0"
                        && additional_model_request_fields
                            .as_ref()
                            .is_some_and(|fields| {
                                fields
                                    .as_object()
                                    .and_then(|map| map.get("thinking"))
                                    .and_then(Document::as_object)
                                    .and_then(|thinking| thinking.get("type"))
                                    .and_then(Document::as_string)
                                    == Some("enabled")
                            })
                        && additional_model_response_field_paths
                            .as_ref()
                            .is_some_and(|paths| paths == &vec!["/thinking".to_string()])
                },
            )
            .returning(|_, _, _, _, _, _, _, _| Ok(response_with_text_and_tool_call()));

        let bedrock = AwsBedrock::builder()
            .test_client(bedrock_mock)
            .default_prompt_model("anthropic.claude-3-5-sonnet-20241022-v2:0")
            .default_options(Options {
                additional_model_request_fields: Some(request_fields),
                additional_model_response_field_paths: Some(vec!["/thinking".to_string()]),
                ..Default::default()
            })
            .build()
            .unwrap();

        let request = ChatCompletionRequest::builder()
            .messages(vec![ChatMessage::User("Hello".into())])
            .build()
            .unwrap();

        let _ = bedrock.complete(&request).await.unwrap();
    }

    #[test_log::test(tokio::test)]
    #[allow(deprecated)]
    async fn test_complete_respects_tool_strict_option() {
        let mut bedrock_mock = MockBedrockConverse::new();

        bedrock_mock
            .expect_converse()
            .once()
            .withf(
                |model_id,
                 _,
                 _,
                 _,
                 tool_config,
                 output_config,
                 _additional_model_request_fields,
                 _additional_model_response_field_paths| {
                    model_id == "anthropic.claude-3-5-sonnet-20241022-v2:0"
                        && output_config.is_none()
                        && tool_config
                            .as_ref()
                            .and_then(|config| config.tools().first())
                            .is_some_and(|tool| match tool {
                                Tool::ToolSpec(spec) => spec.strict() == Some(false),
                                _ => false,
                            })
                },
            )
            .returning(|_, _, _, _, _, _, _, _| Ok(response_with_text_and_tool_call()));

        let bedrock = AwsBedrock::builder()
            .test_client(bedrock_mock)
            .default_prompt_model("anthropic.claude-3-5-sonnet-20241022-v2:0")
            .default_options(Options {
                tool_strict: Some(false),
                ..Default::default()
            })
            .build()
            .unwrap();

        let tool_spec = ToolSpec::builder()
            .name("get_weather")
            .description("Get weather")
            .build()
            .unwrap();
        let request = ChatCompletionRequest::builder()
            .messages(vec![ChatMessage::User("Check weather".into())])
            .tools_spec(HashSet::from([tool_spec]))
            .build()
            .unwrap();

        let _ = bedrock.complete(&request).await.unwrap();
    }

    #[test]
    fn test_tool_config_from_specs_builds_schema() {
        let tool_spec = ToolSpec::builder()
            .name("get_weather")
            .description("Get weather by location")
            .parameters_schema(schema_for!(WeatherArgs))
            .build()
            .unwrap();

        let tool_config = tool_config_from_specs(&HashSet::from([tool_spec]), true)
            .unwrap()
            .expect("tool config");
        assert_eq!(tool_config.tools().len(), 1);

        let spec = match &tool_config.tools()[0] {
            Tool::ToolSpec(spec) => spec,
            _ => panic!("expected tool spec"),
        };

        assert_eq!(spec.name(), "get_weather");
        assert_eq!(spec.description(), Some("Get weather by location"));
        assert_eq!(spec.strict(), Some(true));
        assert!(matches!(
            spec.input_schema(),
            Some(ToolInputSchema::Json(Document::Object(_)))
        ));
    }

    #[test]
    fn test_tool_config_from_specs_can_disable_strict() {
        let tool_spec = ToolSpec::builder()
            .name("get_weather")
            .description("Get weather")
            .build()
            .unwrap();

        let tool_config = tool_config_from_specs(&HashSet::from([tool_spec]), false)
            .unwrap()
            .expect("tool config");

        let spec = match &tool_config.tools()[0] {
            Tool::ToolSpec(spec) => spec,
            _ => panic!("expected tool spec"),
        };

        assert_eq!(spec.strict(), Some(false));
    }

    #[test]
    fn test_response_to_chat_completion_maps_reasoning_content() {
        let response = ConverseOutput::builder()
            .output(ConverseResult::Message(
                Message::builder()
                    .role(ConversationRole::Assistant)
                    .content(ContentBlock::ReasoningContent(
                        ReasoningContentBlock::ReasoningText(
                            ReasoningTextBlock::builder()
                                .text("I should call a weather tool")
                                .signature("sig_123")
                                .build()
                                .unwrap(),
                        ),
                    ))
                    .content(ContentBlock::Text("Working on it".to_string()))
                    .build()
                    .unwrap(),
            ))
            .stop_reason(StopReason::EndTurn)
            .build()
            .unwrap();

        let completion = response_to_chat_completion(&response).unwrap();
        assert_eq!(completion.message.as_deref(), Some("Working on it"));
        let reasoning = completion.reasoning.expect("reasoning items");
        assert_eq!(reasoning.len(), 1);
        assert_eq!(reasoning[0].id, "bedrock_reasoning_0");
        assert_eq!(
            reasoning[0].content.as_ref().and_then(|c| c.first()),
            Some(&"I should call a weather tool".to_string())
        );
        assert_eq!(reasoning[0].encrypted_content.as_deref(), Some("sig_123"));
    }

    #[test]
    fn test_build_converse_input_replays_reasoning_items() {
        let request = ChatCompletionRequest::builder()
            .messages(vec![
                ChatMessage::Reasoning(ReasoningItem {
                    id: "r1".to_string(),
                    summary: Vec::new(),
                    content: Some(vec!["I should call a weather tool".to_string()]),
                    encrypted_content: Some("sig_123".to_string()),
                    status: None,
                }),
                ChatMessage::new_user("Use tool"),
            ])
            .build()
            .unwrap();

        let (messages, _system, _inference, _tool_config) =
            build_converse_input(&request, &Options::default()).unwrap();

        assert_eq!(messages.len(), 2);
        assert!(matches!(messages[0].role(), ConversationRole::Assistant));
        let reasoning = messages[0]
            .content()
            .first()
            .and_then(|content| content.as_reasoning_content().ok())
            .and_then(|content| content.as_reasoning_text().ok())
            .expect("reasoning content");
        assert_eq!(reasoning.text(), "I should call a weather tool");
        assert_eq!(reasoning.signature(), Some("sig_123"));
    }

    #[test]
    fn test_build_converse_input_maps_image_part() {
        let request = ChatCompletionRequest::builder()
            .messages(vec![ChatMessage::new_user_with_parts(vec![
                ChatMessageContentPart::text("Describe this image"),
                ChatMessageContentPart::image("data:image/png;base64,AA=="),
            ])])
            .build()
            .unwrap();

        let (messages, _system, _inference, _tool_config) =
            build_converse_input(&request, &Options::default()).unwrap();
        assert_eq!(messages.len(), 1);
        assert!(matches!(messages[0].role(), ConversationRole::User));
        assert_eq!(messages[0].content().len(), 2);
        let image = messages[0]
            .content()
            .get(1)
            .and_then(|content| content.as_image().ok())
            .expect("image block");
        assert!(matches!(image.format(), ImageFormat::Png));
        assert!(
            image
                .source()
                .is_some_and(aws_sdk_bedrockruntime::types::ImageSource::is_bytes)
        );
    }

    #[test]
    fn test_build_converse_input_maps_audio_part() {
        let request = ChatCompletionRequest::builder()
            .messages(vec![ChatMessage::new_user_with_parts(vec![
                ChatMessageContentPart::text("Transcribe this"),
                ChatMessageContentPart::audio(ChatMessageContentSource::bytes(
                    vec![1_u8, 2_u8, 3_u8],
                    Some("audio/mpeg".to_string()),
                )),
            ])])
            .build()
            .unwrap();

        let (messages, _system, _inference, _tool_config) =
            build_converse_input(&request, &Options::default()).unwrap();
        let audio = messages[0]
            .content()
            .get(1)
            .and_then(|content| content.as_audio().ok())
            .expect("audio block");
        assert!(matches!(audio.format(), AudioFormat::Mp3));
        assert!(
            audio
                .source()
                .is_some_and(aws_sdk_bedrockruntime::types::AudioSource::is_bytes)
        );
    }

    #[test]
    fn test_build_converse_input_maps_video_part() {
        let request = ChatCompletionRequest::builder()
            .messages(vec![ChatMessage::new_user_with_parts(vec![
                ChatMessageContentPart::text("Describe this clip"),
                ChatMessageContentPart::video("s3://bucket/video.mp4"),
            ])])
            .build()
            .unwrap();

        let (messages, _system, _inference, _tool_config) =
            build_converse_input(&request, &Options::default()).unwrap();
        let video = messages[0]
            .content()
            .get(1)
            .and_then(|content| content.as_video().ok())
            .expect("video block");
        assert!(matches!(video.format(), VideoFormat::Mp4));
        assert!(
            video
                .source()
                .is_some_and(aws_sdk_bedrockruntime::types::VideoSource::is_s3_location)
        );
    }

    #[test]
    fn test_build_converse_input_rejects_audio_http_url() {
        let request = ChatCompletionRequest::builder()
            .messages(vec![ChatMessage::new_user_with_parts(vec![
                ChatMessageContentPart::text("Transcribe this"),
                ChatMessageContentPart::audio("https://example.com/audio.mp3"),
            ])])
            .build()
            .unwrap();

        let error = build_converse_input(&request, &Options::default()).unwrap_err();
        assert!(format!("{error}").contains("audio source URL must be data: or s3://"));
    }

    #[test]
    fn test_build_converse_input_rejects_document_without_text() {
        let request = ChatCompletionRequest::builder()
            .messages(vec![ChatMessage::new_user_with_parts(vec![
                ChatMessageContentPart::document(ChatMessageContentSource::bytes(
                    vec![1_u8, 2_u8],
                    Some("text/plain".to_string()),
                )),
            ])])
            .build()
            .unwrap();

        let error = build_converse_input(&request, &Options::default()).unwrap_err();
        assert!(format!("{error}").contains("require at least one text part"));
    }

    #[test]
    fn test_apply_stream_event_accumulates_deltas() {
        let mut response = ChatCompletionResponse::default();
        let mut stop_reason = None;

        apply_stream_event(
            &ConverseStreamOutput::ContentBlockStart(
                ContentBlockStartEvent::builder()
                    .content_block_index(0)
                    .start(ContentBlockStart::ToolUse(
                        ToolUseBlockStart::builder()
                            .tool_use_id("call_1")
                            .name("get_weather")
                            .build()
                            .unwrap(),
                    ))
                    .build()
                    .unwrap(),
            ),
            &mut response,
            &mut stop_reason,
        );

        apply_stream_event(
            &ConverseStreamOutput::ContentBlockDelta(
                ContentBlockDeltaEvent::builder()
                    .content_block_index(0)
                    .delta(ContentBlockDelta::ToolUse(
                        ToolUseBlockDelta::builder()
                            .input("{\"location\":\"Amsterdam\"}")
                            .build()
                            .unwrap(),
                    ))
                    .build()
                    .unwrap(),
            ),
            &mut response,
            &mut stop_reason,
        );

        apply_stream_event(
            &ConverseStreamOutput::ContentBlockDelta(
                ContentBlockDeltaEvent::builder()
                    .content_block_index(1)
                    .delta(ContentBlockDelta::Text("Tool call created".to_string()))
                    .build()
                    .unwrap(),
            ),
            &mut response,
            &mut stop_reason,
        );

        apply_stream_event(
            &ConverseStreamOutput::ContentBlockDelta(
                ContentBlockDeltaEvent::builder()
                    .content_block_index(2)
                    .delta(ContentBlockDelta::ReasoningContent(
                        ReasoningContentBlockDelta::Text("Thinking...".to_string()),
                    ))
                    .build()
                    .unwrap(),
            ),
            &mut response,
            &mut stop_reason,
        );

        apply_stream_event(
            &ConverseStreamOutput::ContentBlockDelta(
                ContentBlockDeltaEvent::builder()
                    .content_block_index(2)
                    .delta(ContentBlockDelta::ReasoningContent(
                        ReasoningContentBlockDelta::Signature("sig_123".to_string()),
                    ))
                    .build()
                    .unwrap(),
            ),
            &mut response,
            &mut stop_reason,
        );

        apply_stream_event(
            &ConverseStreamOutput::Metadata(
                aws_sdk_bedrockruntime::types::ConverseStreamMetadataEvent::builder()
                    .usage(
                        TokenUsage::builder()
                            .input_tokens(5)
                            .output_tokens(3)
                            .total_tokens(8)
                            .build()
                            .unwrap(),
                    )
                    .build(),
            ),
            &mut response,
            &mut stop_reason,
        );

        apply_stream_event(
            &ConverseStreamOutput::MessageStop(
                MessageStopEvent::builder()
                    .stop_reason(StopReason::ToolUse)
                    .build()
                    .unwrap(),
            ),
            &mut response,
            &mut stop_reason,
        );

        assert_eq!(response.message.as_deref(), Some("Tool call created"));
        let tool_call = response
            .tool_calls
            .as_ref()
            .and_then(|calls| calls.first())
            .expect("tool call");
        assert_eq!(tool_call.id(), "call_1");
        assert_eq!(tool_call.name(), "get_weather");
        assert_eq!(tool_call.args(), Some("{\"location\":\"Amsterdam\"}"));
        let reasoning = response.reasoning.expect("reasoning item");
        assert_eq!(reasoning.len(), 1);
        assert_eq!(reasoning[0].id, "bedrock_reasoning_2");
        assert_eq!(
            reasoning[0].content.as_ref().and_then(|c| c.first()),
            Some(&"Thinking...".to_string())
        );
        assert_eq!(reasoning[0].encrypted_content.as_deref(), Some("sig_123"));
        assert_eq!(response.usage.unwrap().total_tokens, 8);
        assert!(matches!(stop_reason, Some(StopReason::ToolUse)));
    }
}
