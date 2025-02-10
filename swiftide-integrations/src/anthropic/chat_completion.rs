use anyhow::{Context as _, Result};
use async_anthropic::types::{
    CreateMessagesRequestBuilder, Message, MessageBuilder, MessageContent, MessageContentList,
    MessageRole, ToolChoice, ToolResultBuilder, ToolUseBuilder,
};
use async_trait::async_trait;
use serde_json::json;
use swiftide_core::{
    chat_completion::{
        errors::ChatCompletionError, ChatCompletionRequest, ChatCompletionResponse, ChatMessage,
        ToolCall, ToolSpec,
    },
    ChatCompletion,
};

use super::Anthropic;

#[async_trait]
impl ChatCompletion for Anthropic {
    #[tracing::instrument(skip_all, err)]
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
            .map(message_to_antropic)
            .collect::<Result<Vec<_>>>()?;

        let mut anthropic_request = CreateMessagesRequestBuilder::default()
            .model(model)
            .messages(messages)
            .to_owned();

        if !request.tools_spec.is_empty() {
            anthropic_request
                .tools(
                    request
                        .tools_spec()
                        .iter()
                        .map(tools_to_anthropic)
                        .collect::<Result<Vec<_>>>()?,
                )
                .tool_choice(ToolChoice::Auto);
        }

        let request = anthropic_request
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
            .flat_map(Message::tool_uses)
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

#[allow(clippy::items_after_statements)]
fn message_to_antropic(message: &ChatMessage) -> Result<Message> {
    let mut builder = MessageBuilder::default().role(MessageRole::User).to_owned();

    use ChatMessage::{Assistant, Summary, System, ToolOutput, User};

    match message {
        ToolOutput(tool_call, tool_output) => builder.content(
            ToolResultBuilder::default()
                .tool_use_id(tool_call.id())
                .content(tool_output.content().unwrap_or("Success"))
                .build()?,
        ),
        Summary(msg) | System(msg) | User(msg) => builder.content(msg),
        Assistant(msg, tool_calls) => {
            builder.role(MessageRole::Assistant);

            let mut content_list: Vec<MessageContent> = Vec::new();

            if let Some(msg) = msg {
                content_list.push(msg.into());
            }

            if let Some(tool_calls) = tool_calls {
                for tool_call in tool_calls {
                    let tool_call = ToolUseBuilder::default()
                        .id(tool_call.id())
                        .name(tool_call.name())
                        .input(tool_call.args())
                        .build()?;

                    content_list.push(tool_call.into());
                }
            }

            let content_list = MessageContentList(content_list);

            builder.content(content_list)
        }
    };

    builder.build().context("Failed to build message")
}

fn tools_to_anthropic(
    spec: &ToolSpec,
) -> Result<serde_json::value::Map<String, serde_json::Value>> {
    let properties = spec
        .parameters
        .iter()
        .map(|param| {
            let map = json!({
                param.name: {
                    "type": "string",
                    "description": param.description,
                }
            })
            .as_object()
            .context("Failed to build tool")?
            .to_owned();

            Ok(map)
        })
        .collect::<Result<Vec<_>>>()?;
    let map = json!({
        "name": spec.name,
        "description": spec.description,
        "input_schema": {
            "type": "object",
            "properties": properties,
        },
        "required": spec.parameters.iter().filter(|param| param.required).map(|param| param.name).collect::<Vec<_>>(),
    })
    .as_object_mut()
    .context("Failed to build tool")?
    .to_owned();

    Ok(map)
}
