use anyhow::{Context as _, Result};
use async_openai::types::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestToolMessageArgs, ChatCompletionRequestUserMessageArgs,
    CreateChatCompletionRequestArgs,
};
use async_trait::async_trait;
use itertools::Itertools;
use swiftide_core::chat_completion::{
    ChatCompletion, ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ToolCall,
    ToolOutput,
};

use super::OpenAI;

#[async_trait]
impl ChatCompletion for OpenAI {
    async fn complete(
        &self,
        request: impl Into<ChatCompletionRequest<'_>> + Send + Sync,
    ) -> Result<ChatCompletionResponse> {
        let request: ChatCompletionRequest = request.into();

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
            .message(
                response
                    .choices
                    .first()
                    .and_then(|choice| choice.message.content.clone()),
            )
            .tool_calls(
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
                                    .arguments(tool_call.function.arguments.clone())
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
        ChatMessage::ToolOuput(tool_output) => {
            // get the id and content or return None
            let (Some(id), Some(content)) = (tool_output.tool_call_id(), tool_output.content())
            else {
                return Ok(None);
            };

            Some(
                ChatCompletionRequestToolMessageArgs::default()
                    .content(id)
                    .tool_call_id(content)
                    .build()?
                    .into(),
            )
        }
    };

    Ok(openai_message)
}
