use std::collections::{HashMap, HashSet};

use anyhow::Context as _;
use async_trait::async_trait;
use aws_sdk_bedrockruntime::{
    operation::converse::ConverseOutput,
    types::{
        AutoToolChoice, ContentBlock, ContentBlockDelta, ContentBlockStart, ConversationRole,
        ConverseOutput as ConverseResult, ConverseStreamOutput, InferenceConfiguration, Message,
        StopReason, SystemContentBlock, Tool, ToolChoice, ToolConfiguration, ToolInputSchema,
        ToolResultBlock, ToolResultContentBlock, ToolResultStatus, ToolSpecification, ToolUseBlock,
    },
};
use aws_smithy_types::Document;
use futures_util::stream;
use swiftide_core::{
    ChatCompletion, ChatCompletionStream,
    chat_completion::{
        ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ChatMessageContentPart,
        ToolCall, ToolOutput, ToolSpec, errors::LanguageModelError,
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
        request: &ChatCompletionRequest,
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
            )
            .await?;

        tracing::debug!(response = ?response, "[ChatCompletion] Response from bedrock converse");

        let completion = response_to_chat_completion(&response)?;

        if completion.message.is_none()
            && completion.tool_calls.is_none()
            && super::is_context_length_stop_reason(response.stop_reason())
        {
            return Err(LanguageModelError::context_length_exceeded(
                "Model context window exceeded",
            ));
        }

        if let Some(usage) = completion.usage.as_ref() {
            self.report_usage(model, usage).await?;
        }

        Ok(completion)
    }

    #[tracing::instrument(skip_all)]
    async fn complete_stream(&self, request: &ChatCompletionRequest) -> ChatCompletionStream {
        let model = match self.prompt_model() {
            Ok(model) => model.to_string(),
            Err(error) => return error.into(),
        };

        #[cfg(not(feature = "metrics"))]
        let _ = &model;

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
            )
            .await
        {
            Ok(stream_output) => stream_output,
            Err(error) => return error.into(),
        };

        let on_usage = self.on_usage.clone();
        #[cfg(feature = "metrics")]
        let metric_metadata = self.metric_metadata.clone();

        let event_stream = stream_output.stream;
        let stream = stream::unfold(
            (
                event_stream,
                ChatCompletionResponse::default(),
                None::<StopReason>,
                false,
            ),
            move |(mut event_stream, mut response, mut stop_reason, finished)| {
                let on_usage = on_usage.clone();
                let model = model.clone();
                #[cfg(not(feature = "metrics"))]
                let _ = &model;
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
                                (event_stream, response, stop_reason, false),
                            ))
                        }
                        Ok(None) => {
                            if response.message.is_none()
                                && response.tool_calls.is_none()
                                && stop_reason
                                    .as_ref()
                                    .is_some_and(super::is_context_length_stop_reason)
                            {
                                return Some((
                                    Err(LanguageModelError::context_length_exceeded(
                                        "Model context window exceeded",
                                    )),
                                    (event_stream, response, stop_reason, true),
                                ));
                            }

                            if let Some(usage) = response.usage.as_ref() {
                                if let Some(callback) = on_usage.as_ref()
                                    && let Err(error) = callback(usage).await
                                {
                                    return Some((
                                        Err(LanguageModelError::permanent(error)),
                                        (event_stream, response, stop_reason, true),
                                    ));
                                }

                                #[cfg(feature = "metrics")]
                                emit_usage(
                                    &model,
                                    usage.prompt_tokens.into(),
                                    usage.completion_tokens.into(),
                                    usage.total_tokens.into(),
                                    metric_metadata.as_ref(),
                                );
                            }

                            Some((
                                Ok(response.clone()),
                                (event_stream, response, stop_reason, true),
                            ))
                        }
                        Err(error) => Some((
                            Err(super::converse_stream_output_error_to_language_model_error(
                                error,
                            )),
                            (event_stream, response, stop_reason, true),
                        )),
                    }
                }
            },
        );

        Box::pin(stream)
    }
}

fn build_converse_input(
    request: &ChatCompletionRequest,
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
            ChatMessage::System(text) => system.push(SystemContentBlock::Text(text.clone())),
            ChatMessage::Summary(text) => messages.push(user_message_from_text(text.clone())?),
            ChatMessage::User(text) => messages.push(user_message_from_text(text.clone())?),
            ChatMessage::UserWithParts(parts) => messages.push(user_message_from_parts(parts)?),
            ChatMessage::Assistant(content, maybe_tool_calls) => {
                let mut blocks = Vec::with_capacity(
                    usize::from(content.as_ref().is_some_and(|text| !text.is_empty()))
                        + maybe_tool_calls.as_ref().map_or(0, Vec::len),
                );

                if let Some(content) = content.as_ref()
                    && !content.is_empty()
                {
                    blocks.push(ContentBlock::Text(content.clone()));
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
                    messages.push(assistant_message_from_blocks(blocks)?);
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

                messages.push(user_message_from_blocks(vec![ContentBlock::ToolResult(
                    tool_result,
                )])?);
            }
            ChatMessage::Reasoning(_) => {}
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
        tool_config_from_specs(request.tools_spec())?,
    ))
}

fn user_message_from_text(text: String) -> Result<Message, LanguageModelError> {
    user_message_from_blocks(vec![ContentBlock::Text(text)])
}

fn user_message_from_parts(
    parts: &[ChatMessageContentPart],
) -> Result<Message, LanguageModelError> {
    let mut text = String::new();
    for part in parts {
        match part {
            ChatMessageContentPart::Text { text: part_text } => {
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(part_text);
            }
            ChatMessageContentPart::ImageUrl { .. } => {
                return Err(LanguageModelError::permanent(
                    "Bedrock chat completions do not support image_url inputs yet",
                ));
            }
        }
    }

    if text.is_empty() {
        return Err(LanguageModelError::permanent(
            "UserWithParts requires at least one text part",
        ));
    }

    user_message_from_text(text)
}

fn user_message_from_blocks(blocks: Vec<ContentBlock>) -> Result<Message, LanguageModelError> {
    Message::builder()
        .role(ConversationRole::User)
        .set_content(Some(blocks))
        .build()
        .map_err(LanguageModelError::permanent)
}

fn assistant_message_from_blocks(blocks: Vec<ContentBlock>) -> Result<Message, LanguageModelError> {
    Message::builder()
        .role(ConversationRole::Assistant)
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
            Ok(ToolResultContentBlock::Json(
                serde_json::from_value(value.clone()).map_err(LanguageModelError::permanent)?,
            ))
        }
        _ => Ok(ToolResultContentBlock::Text(output.to_string())),
    }
}

fn tool_call_args_to_document(args: Option<&str>) -> Result<Document, LanguageModelError> {
    match args.map(str::trim) {
        Some(args) if !args.is_empty() => serde_json::from_str(args)
            .with_context(|| format!("Failed to parse tool args as JSON: {args}"))
            .map_err(LanguageModelError::permanent),
        _ => Ok(Document::Object(HashMap::new())),
    }
}

fn tool_config_from_specs(
    tool_specs: &HashSet<ToolSpec>,
) -> Result<Option<ToolConfiguration>, LanguageModelError> {
    if tool_specs.is_empty() {
        return Ok(None);
    }

    let tools = tool_specs
        .iter()
        .map(tool_spec_to_bedrock)
        .collect::<Result<Vec<_>, _>>()?;

    let tool_config = ToolConfiguration::builder()
        .set_tools(Some(tools))
        .tool_choice(ToolChoice::Auto(AutoToolChoice::builder().build()))
        .build()
        .map_err(LanguageModelError::permanent)?;

    Ok(Some(tool_config))
}

fn tool_spec_to_bedrock(spec: &ToolSpec) -> Result<Tool, LanguageModelError> {
    let schema_value = match spec.parameters_schema.as_ref() {
        Some(schema) => serde_json::to_value(schema).map_err(LanguageModelError::permanent)?,
        None => serde_json::json!({
            "type": "object",
            "properties": {}
        }),
    };
    let input_schema = ToolInputSchema::Json(
        serde_json::from_value(schema_value).map_err(LanguageModelError::permanent)?,
    );

    let mut builder = ToolSpecification::builder()
        .name(spec.name.clone())
        .input_schema(input_schema);

    if !spec.description.is_empty() {
        builder = builder.description(spec.description.clone());
    }

    let tool_spec = builder.build().map_err(LanguageModelError::permanent)?;
    Ok(Tool::ToolSpec(tool_spec))
}

fn response_to_chat_completion(
    response: &ConverseOutput,
) -> Result<ChatCompletionResponse, LanguageModelError> {
    let (message, tool_calls) = match response.output() {
        Some(output) => match output {
            ConverseResult::Message(message) => extract_message_and_tool_calls(message)?,
            _ => (None, None),
        },
        None => (None, None),
    };

    let mut builder = ChatCompletionResponse::builder()
        .maybe_message(message)
        .maybe_tool_calls(tool_calls)
        .to_owned();

    if let Some(usage) = response.usage() {
        builder.usage(super::usage_from_bedrock(usage));
    }

    builder.build().map_err(LanguageModelError::from)
}

fn extract_message_and_tool_calls(
    message: &Message,
) -> Result<(Option<String>, Option<Vec<ToolCall>>), LanguageModelError> {
    let mut text = String::new();
    let mut has_text = false;
    let mut tool_calls = Vec::with_capacity(message.content().len());

    for block in message.content() {
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
            _ => {}
        }
    }

    let message = has_text.then_some(text);
    let tool_calls = (!tool_calls.is_empty()).then_some(tool_calls);

    Ok((message, tool_calls))
}

fn document_to_json_string(document: &Document) -> Result<String, LanguageModelError> {
    serde_json::to_string(document).map_err(LanguageModelError::permanent)
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

#[cfg(test)]
mod tests {
    use aws_sdk_bedrockruntime::{
        operation::converse::ConverseOutput,
        types::{
            ContentBlockDeltaEvent, ContentBlockStart, ContentBlockStartEvent,
            ConverseOutput as ConverseResult, Message, MessageStopEvent, StopReason, TokenUsage,
            ToolUseBlockDelta, ToolUseBlockStart,
        },
    };
    use schemars::{JsonSchema, schema_for};
    use swiftide_core::chat_completion::{ChatMessage, ToolSpec};

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
                |model_id, messages, system, inference_config, tool_config| {
                    model_id == "anthropic.claude-3-5-sonnet-20241022-v2:0"
                        && messages.len() == 1
                        && system.is_none()
                        && inference_config.is_none()
                        && tool_config.is_none()
                },
            )
            .returning(|_, _, _, _, _| Ok(response_with_text_and_tool_call()));

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

    #[test]
    fn test_tool_config_from_specs_builds_schema() {
        let tool_spec = ToolSpec::builder()
            .name("get_weather")
            .description("Get weather by location")
            .parameters_schema(schema_for!(WeatherArgs))
            .build()
            .unwrap();

        let tool_config = tool_config_from_specs(&HashSet::from([tool_spec]))
            .unwrap()
            .expect("tool config");
        assert_eq!(tool_config.tools().len(), 1);

        let spec = match &tool_config.tools()[0] {
            Tool::ToolSpec(spec) => spec,
            _ => panic!("expected tool spec"),
        };

        assert_eq!(spec.name(), "get_weather");
        assert_eq!(spec.description(), Some("Get weather by location"));
        assert!(matches!(
            spec.input_schema(),
            Some(ToolInputSchema::Json(Document::Object(_)))
        ));
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
        assert_eq!(response.usage.unwrap().total_tokens, 8);
        assert!(matches!(stop_reason, Some(StopReason::ToolUse)));
    }
}
