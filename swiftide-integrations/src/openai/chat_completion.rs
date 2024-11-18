use anyhow::{Context as _, Result};
use async_openai::types::{
    ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestToolMessageArgs, ChatCompletionRequestUserMessageArgs,
    CreateChatCompletionRequestArgs,
};
use async_trait::async_trait;
use itertools::Itertools;
use swiftide_core::chat_completion::{
    ChatCompletion, ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ToolCall,
};

use super::OpenAI;

#[async_trait]
impl ChatCompletion for OpenAI {
    async fn complete(&self, request: &ChatCompletionRequest) -> Result<ChatCompletionResponse> {
        let model = self
            .default_options
            .prompt_model
            .as_ref()
            .context("Model not set")?;

        let messages = request
            .messages()
            .iter()
            .map(message_to_openai)
            .filter_map_ok(|msg| msg)
            .collect::<Result<Vec<_>>>()?;

        // Build the request to be sent to the OpenAI API.
        let request = CreateChatCompletionRequestArgs::default()
            .model(model)
            .messages(messages)
            .build()?;

        let response = self
            .client
            .chat()
            .create(request)
            .await
            .context("Completion request to openai failed")?;

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
                                    .expect("Building tool call failed, should never happen")
                            })
                            .collect_vec()
                    }),
            )
            .build()
    }
}

fn message_to_openai(
    message: &ChatMessage,
) -> Result<Option<async_openai::types::ChatCompletionRequestMessage>> {
    let openai_message = match message {
        ChatMessage::User(msg) => Some(
            ChatCompletionRequestUserMessageArgs::default()
                .content(msg.as_str())
                .build()?
                .into(),
        ),
        ChatMessage::System(msg) => Some(
            ChatCompletionRequestSystemMessageArgs::default()
                .content(msg.as_str())
                .build()?
                .into(),
        ),
        ChatMessage::ToolCall(_) => None,
        ChatMessage::ToolOutput(tool_call, tool_output) => {
            let Some(content) = tool_output.content() else {
                return Ok(None);
            };

            Some(
                ChatCompletionRequestToolMessageArgs::default()
                    .content(content)
                    .tool_call_id(tool_call.id())
                    .build()?
                    .into(),
            )
        }
        ChatMessage::Assistant(msg) => Some(
            ChatCompletionRequestAssistantMessageArgs::default()
                .content(msg.as_str())
                .build()?
                .into(),
        ),
    };

    Ok(openai_message)
}