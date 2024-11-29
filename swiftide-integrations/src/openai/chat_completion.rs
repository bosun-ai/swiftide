use anyhow::{Context as _, Result};
use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessageArgs,
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestToolMessageArgs,
    ChatCompletionRequestUserMessageArgs, ChatCompletionTool, ChatCompletionToolArgs,
    ChatCompletionToolType, CreateChatCompletionRequestArgs, FunctionCall, FunctionObjectArgs,
};
use async_trait::async_trait;
use itertools::Itertools;
use serde_json::json;
use swiftide_core::chat_completion::{
    errors::ChatCompletionError, ChatCompletion, ChatCompletionRequest, ChatCompletionResponse,
    ChatMessage, ToolCall, ToolSpec,
};

use super::OpenAI;

#[async_trait]
impl ChatCompletion for OpenAI {
    #[tracing::instrument(skip_all)]
    async fn complete(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, ChatCompletionError> {
        let model = self
            .default_options
            .prompt_model
            .as_ref()
            .context("Model not set")?;

        let messages = request
            .messages()
            .iter()
            .map(message_to_openai)
            .collect::<Result<Vec<_>>>()?;

        // Build the request to be sent to the OpenAI API.
        let request = CreateChatCompletionRequestArgs::default()
            .model(model)
            .messages(messages)
            .tools(
                request
                    .tools_spec()
                    .iter()
                    .map(tools_to_openai)
                    .collect::<Result<Vec<_>>>()?,
            )
            .tool_choice("auto")
            .parallel_tool_calls(true)
            .build()
            .map_err(|e| ChatCompletionError::LLM(Box::new(e)))?;

        tracing::debug!(
            request = serde_json::to_string_pretty(&request).expect("infallible"),
            "Sending request to OpenAI"
        );

        let response = self
            .client
            .chat()
            .create(request)
            .await
            .map_err(|e| ChatCompletionError::LLM(Box::new(e)))?;

        tracing::debug!(
            response = serde_json::to_string_pretty(&response).expect("infallible"),
            "Received response from OpenAI"
        );

        ChatCompletionResponse::builder()
            .maybe_message(
                response
                    .choices
                    .first()
                    .and_then(|choice| choice.message.content.clone()),
            )
            .maybe_tool_calls(
                response
                    .choices
                    .first()
                    .and_then(|choice| choice.message.tool_calls.clone())
                    .map(|tool_calls| {
                        tool_calls
                            .iter()
                            .map(|tool_call| {
                                ToolCall::builder()
                                    .id(tool_call.id.clone())
                                    .args(tool_call.function.arguments.clone())
                                    .name(tool_call.function.name.clone())
                                    .build()
                                    .expect("infallible")
                            })
                            .collect_vec()
                    }),
            )
            .build()
            .map_err(ChatCompletionError::from)
    }
}

// TODO: Maybe just into the whole thing? Types are not in this crate

fn tools_to_openai(spec: &ToolSpec) -> Result<ChatCompletionTool> {
    let mut properties = serde_json::Map::new();

    for param in &spec.parameters {
        properties.insert(
            param.name.to_string(),
            json!({
                "type": "string",
                "description": param.description,
            }),
        );
    }

    ChatCompletionToolArgs::default()
        .r#type(ChatCompletionToolType::Function)
        .function(FunctionObjectArgs::default()
            .name(spec.name)
            .description(spec.description)
            .parameters(json!({
                "type": "object",
                "properties": properties,
                "required": spec.parameters.iter().filter(|param| param.required).map(|param| param.name).collect_vec(),
                "additionalProperties": false,
            })).build()?).build()
        .map_err(anyhow::Error::from)
}

fn message_to_openai(
    message: &ChatMessage,
) -> Result<async_openai::types::ChatCompletionRequestMessage> {
    let openai_message = match message {
        ChatMessage::User(msg) => ChatCompletionRequestUserMessageArgs::default()
            .content(msg.as_str())
            .build()?
            .into(),
        ChatMessage::System(msg) => ChatCompletionRequestSystemMessageArgs::default()
            .content(msg.as_str())
            .build()?
            .into(),
        ChatMessage::ToolOutput(tool_call, tool_output) => {
            let Some(content) = tool_output.content() else {
                return Ok(ChatCompletionRequestToolMessageArgs::default()
                    .tool_call_id(tool_call.id())
                    .build()?
                    .into());
            };

            ChatCompletionRequestToolMessageArgs::default()
                .content(content)
                .tool_call_id(tool_call.id())
                .build()?
                .into()
        }
        ChatMessage::Assistant(msg, tool_calls) => {
            let mut builder = ChatCompletionRequestAssistantMessageArgs::default();

            if let Some(msg) = msg {
                builder.content(msg.as_str());
            }

            if let Some(tool_calls) = tool_calls {
                builder.tool_calls(
                    tool_calls
                        .iter()
                        .map(|tool_call| {
                            let mut truncated_args =
                                tool_call.args().unwrap_or_default().to_string();

                            truncated_args.truncate(100);

                            ChatCompletionMessageToolCall {
                                id: tool_call.id().to_string(),
                                r#type: ChatCompletionToolType::Function,
                                function: FunctionCall {
                                    name: tool_call.name().to_string(),
                                    arguments: truncated_args,
                                },
                            }
                        })
                        .collect::<Vec<_>>(),
                );
            }

            builder.build()?.into()
        }
    };

    Ok(openai_message)
}
