use anyhow::{Context as _, Result};
use async_anthropic::types::{
    CreateMessagesRequestBuilder, CreateMessagesResponseBuilder, Message, MessageBuilder,
    MessageContent, MessageContentList, MessageRole, ToolChoice, ToolResultBuilder,
};
use async_trait::async_trait;
use swiftide_core::{
    chat_completion::{errors::ChatCompletionError, ChatCompletionResponse, ChatMessage, ToolCall},
    ChatCompletion,
};

use super::Anthropic;

#[async_trait]
impl ChatCompletion for Anthropic {
    #[tracing::instrument(skip_all, err)]
    async fn complete(
        &self,
        request: &swiftide_core::chat_completion::ChatCompletionRequest,
    ) -> Result<swiftide_core::chat_completion::ChatCompletionResponse, ChatCompletionError> {
        let model = self
            .default_options
            .prompt_model
            .as_ref()
            .context("Model not set")?;

        let messages = request
            .messages()
            .iter()
            .map(message_to_anthropic)
            .collect::<Result<Vec<_>>>()?;

        let mut anthropic_requerst = CreateMessagesRequestBuilder::default()
            .model(model)
            .messages(messages);

        if !request.tools_spec.is_empty() {
            anthropic_requerst = anthropic_requerst
                .tools(
                    request
                        .tools_spec()
                        .iter()
                        .map(tools_to_anthropic)
                        .collect::<Result<Vec<_>>>()?,
                )
                .tool_choice(ToolChoice::Auto);
        }

        let request = anthropic_requerst
            .build()
            .map_err(|e| ChatCompletionError::LLM(Box::new(e)))?;

        tracing::debug!(
            model = &model,
            messages = serde_json::to_string_pretty(&request).expect("Infallible"),
            "[ChatCompletion] Request to anthropic"
        );

        let response = self
            .client
            .messages()
            .create(request)
            .await
            .map_err(|e| ChatCompletionError::LLM(Box::new(e)))?;

        tracing::debug!(
            response = serde_json::to_string_pretty(&response).expect("Infallible"),
            "[ChatCompletion] Response from anthropic"
        );

        let maybe_tool_calls = response
            .messages()
            .iter()
            .map(Message::tool_uses)
            .flatten()
            .map(|atool| {
                ToolCall::builder()
                    .id(atool.id)
                    .name(atool.name)
                    .args(atool.input.to_string())
                    .build()
                    .expect("infallible")
            })
            .collect::<Vec<_>>();
        let maybe_tool_calls = if maybe_tool_calls.is_empty() {
            None
        } else {
            Some(maybe_tool_calls)
        };

        ChatCompletionResponse::builder()
            .maybe_message(response.messages().iter().find_map(Message::text))
            .maybe_tool_calls(maybe_tool_calls)
            .build()
            .map_err(ChatCompletionError::from)
    }
}

fn message_to_antropic(message: &ChatMessage) -> Result<Message> {
    let mut builder = MessageBuilder::default().role(MessageRole::User).to_owned();

    use ChatMessage::*;

    match message {
        System(msg) => builder.content(msg),
        User(msg) => builder.content(msg),
        ToolOutput(tool_call, tool_output) => builder.content(
            ToolResultBuilder::default()
                .tool_use_id(tool_call.id().clone())
                .content(tool_output.content().unwrap_or("Success"))
                .build()?,
        ),
        Summary(msg) => builder.content(msg),
        Assistant(msg, tool_calls) => {
            builder.role(MessageRole::Assistant);

            let mut content_list: Vec<MessageContent> = Vec::new();

            if let Some(msg) = msg {
                content_list.push(msg.into());
            }

            let content = MessageContentList(content_list);

            builder.content(content_list)
        }
    };

    builder.build().context("Failed to build message")
}
