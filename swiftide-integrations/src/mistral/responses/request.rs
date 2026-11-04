use async_openai::types::responses as r;
use base64::Engine as _;
use schemars::Schema;
use serde_json::Value;
use swiftide_core::chat_completion::{
    ChatCompletionRequest, ChatMessage, ChatMessageContentPart, ChatMessageContentSource,
    ToolOutput, errors::LanguageModelError,
};

use super::{Mistral, Result, reasoning_to_sdk};

impl Mistral {
    pub(super) fn responses_request(
        &self,
        request: &ChatCompletionRequest<'_>,
        streaming: bool,
        schema: Option<Schema>,
    ) -> Result<Value> {
        let options = &self.default_options;
        let model = options
            .prompt_model
            .as_deref()
            .ok_or_else(|| LanguageModelError::permanent("Model not set"))?;
        if options.random_seed.is_some() {
            return Err(LanguageModelError::permanent(
                "random_seed is not supported by Mistral Responses",
            ));
        }
        let mut args = r::CreateResponseArgs::default();
        args.model(model)
            .input(r::InputParam::Items(input_items(request.messages())?))
            .store(false)
            .stream(streaming);
        if let Some(value) = options.max_tokens {
            args.max_output_tokens(value);
        }
        if let Some(value) = options.temperature {
            args.temperature(value);
        }
        if let Some(value) = options.top_p {
            args.top_p(value);
        }
        if let Some(value) = options.parallel_tool_calls {
            args.parallel_tool_calls(value);
        }
        if let Some(value) = &options.reasoning_effort {
            args.reasoning(
                r::ReasoningArgs::default()
                    .effort(value.clone())
                    .build()
                    .map_err(LanguageModelError::permanent)?,
            );
        }
        if !request.tools_spec().is_empty() {
            let tools = request
                .tools_spec()
                .iter()
                .map(|spec| {
                    Ok(r::Tool::Function(r::FunctionTool {
                        name: spec.name.clone(),
                        description: Some(spec.description.clone()),
                        parameters: Some(
                            spec.canonical_parameters_schema_json()
                                .map_err(LanguageModelError::permanent)?,
                        ),
                        strict: None,
                        defer_loading: None,
                    }))
                })
                .collect::<Result<Vec<_>>>()?;
            args.tools(tools)
                .tool_choice(r::ToolChoiceParam::Mode(r::ToolChoiceOptions::Auto));
        }
        let mut body = serde_json::to_value(args.build().map_err(LanguageModelError::permanent)?)
            .map_err(LanguageModelError::permanent)?;
        body.as_object_mut()
            .expect("request object")
            .extend(options.extra_body.clone());
        // These endpoints cannot maintain server-side state. Reject incompatible overrides;
        // never silently enable storage or truncate a caller's history.
        if body["store"] == true
            || body["background"] == true
            || !body["conversation"].is_null()
            || !body["previous_response_id"].is_null()
            || body
                .get("truncation")
                .is_some_and(|value| value != "disabled")
            || body["stream"] != streaming
        {
            return Err(LanguageModelError::permanent(
                "Mistral Responses requires stateless, non-truncating requests with the selected stream mode",
            ));
        }
        if let Some(schema) = schema {
            body["text"] = serde_json::to_value(r::ResponseTextParam {
                format: r::TextResponseFormatConfiguration::JsonSchema(
                    r::ResponseFormatJsonSchema {
                        description: None,
                        name: "structured_prompt".into(),
                        schema: schema.into(),
                        strict: Some(true),
                    },
                ),
                verbosity: None,
            })
            .map_err(LanguageModelError::permanent)?;
        }
        Ok(body)
    }
}

fn message(role: r::Role, content: r::EasyInputContent) -> Result<r::InputItem> {
    Ok(r::InputItem::EasyMessage(
        r::EasyInputMessageArgs::default()
            .r#type(r::MessageType::Message)
            .role(role)
            .content(content)
            .build()
            .map_err(LanguageModelError::permanent)?,
    ))
}

fn input_items(messages: &[ChatMessage]) -> Result<Vec<r::InputItem>> {
    let mut items = Vec::new();
    for msg in messages {
        match msg {
            ChatMessage::System(text) => items.push(message(
                r::Role::System,
                r::EasyInputContent::Text(text.clone()),
            )?),
            ChatMessage::User(text) => items.push(message(
                r::Role::User,
                r::EasyInputContent::Text(text.clone()),
            )?),
            ChatMessage::Summary(text) => items.push(message(
                r::Role::Assistant,
                r::EasyInputContent::Text(text.clone()),
            )?),
            ChatMessage::UserWithParts(parts) => items.push(message(
                r::Role::User,
                r::EasyInputContent::ContentList(
                    parts.iter().map(input_part).collect::<Result<Vec<_>>>()?,
                ),
            )?),
            ChatMessage::Reasoning(item) => items.push(r::InputItem::Item(r::Item::Reasoning(
                reasoning_to_sdk(item),
            ))),
            ChatMessage::Assistant(text, calls) => {
                if let Some(text) = text {
                    items.push(message(
                        r::Role::Assistant,
                        r::EasyInputContent::Text(text.clone()),
                    )?);
                }
                for call in calls.iter().flatten() {
                    items.push(r::InputItem::Item(r::Item::FunctionCall(
                        r::FunctionToolCall {
                            call_id: call.id().to_owned(),
                            name: call.name().to_owned(),
                            arguments: call.args().unwrap_or_default().to_owned(),
                            namespace: None,
                            id: None,
                            status: Some(r::OutputStatus::Completed),
                        },
                    )));
                }
            }
            ChatMessage::ToolOutput(call, output) => {
                let output = match output {
                    ToolOutput::Text(text) | ToolOutput::Fail(text) => text.clone(),
                    ToolOutput::FeedbackRequired(value)
                    | ToolOutput::Stop(value)
                    | ToolOutput::AgentFailed(value) => {
                        value.as_ref().map_or_else(String::new, Value::to_string)
                    }
                    _ => {
                        return Err(LanguageModelError::permanent(
                            "Unsupported tool output for Mistral Responses",
                        ));
                    }
                };
                items.push(r::InputItem::Item(r::Item::FunctionCallOutput(
                    r::FunctionCallOutputItemParam {
                        call_id: call.id().to_owned(),
                        output: r::FunctionCallOutput::Text(output),
                        id: None,
                        status: Some(r::OutputStatus::Completed),
                    },
                )));
            }
        }
    }
    Ok(items)
}

fn input_part(part: &ChatMessageContentPart) -> Result<r::InputContent> {
    Ok(match part {
        ChatMessageContentPart::Text { text } => r::InputTextContent::from(text.as_str()).into(),
        ChatMessageContentPart::Image { source, .. } => {
            let mut image = r::InputImageContent {
                detail: r::ImageDetail::Auto,
                file_id: None,
                image_url: None,
            };
            match source {
                ChatMessageContentSource::Url { url } => image.image_url = Some(url.clone()),
                ChatMessageContentSource::FileId { file_id } => {
                    image.file_id = Some(file_id.clone());
                }
                ChatMessageContentSource::Bytes { data, media_type } => {
                    image.image_url = Some(format!(
                        "data:{};base64,{}",
                        media_type.as_deref().unwrap_or("application/octet-stream"),
                        base64::engine::general_purpose::STANDARD.encode(data)
                    ));
                }
                ChatMessageContentSource::S3 { .. } => {
                    return Err(LanguageModelError::permanent(
                        "Mistral Responses does not support S3 image sources",
                    ));
                }
            }
            image.into()
        }
        ChatMessageContentPart::Document { source, name, .. } => {
            let mut file = r::InputFileArgs::default();
            match source {
                ChatMessageContentSource::Url { url } => {
                    file.file_url(url);
                }
                ChatMessageContentSource::FileId { file_id } => {
                    file.file_id(file_id);
                }
                ChatMessageContentSource::Bytes { data, .. } => {
                    file.file_data(base64::engine::general_purpose::STANDARD.encode(data))
                        .filename(name.as_deref().unwrap_or("document"));
                }
                ChatMessageContentSource::S3 { .. } => {
                    return Err(LanguageModelError::permanent(
                        "Mistral Responses does not support S3 file sources",
                    ));
                }
            }
            file.build().map_err(LanguageModelError::permanent)?.into()
        }
        ChatMessageContentPart::Audio { .. } | ChatMessageContentPart::Video { .. } => {
            return Err(LanguageModelError::permanent(
                "Mistral Responses supports text, images and files, not audio/video parts",
            ));
        }
    })
}
