use futures_util::{StreamExt as _, TryStreamExt as _, stream};
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result};
use async_anthropic::types::{
    CreateMessagesRequestBuilder, Message, MessageBuilder, MessageContent, MessageContentList,
    MessageRole, MessagesStreamEvent, ToolChoice, ToolResultBuilder, ToolUseBuilder,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use swiftide_core::{
    ChatCompletion, ChatCompletionStream,
    chat_completion::{
        ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ToolCall, ToolSpec, Usage,
        UsageBuilder, errors::LanguageModelError,
    },
};

use super::Anthropic;

#[cfg(feature = "metrics")]
use swiftide_core::metrics::emit_usage;

#[async_trait]
impl ChatCompletion for Anthropic {
    #[tracing::instrument(skip_all, err)]
    async fn complete(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, LanguageModelError> {
        let model = &self.default_options.prompt_model;
        let request = self
            .build_request(request)
            .and_then(|b| b.build().map_err(LanguageModelError::permanent))?;

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
            .map_err(LanguageModelError::permanent)?;

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

        let mut builder = ChatCompletionResponse::builder()
            .maybe_message(response.messages().iter().find_map(Message::text))
            .maybe_tool_calls(maybe_tool_calls)
            .to_owned();

        if let Some(usage) = &response.usage {
            let input_tokens = usage.input_tokens.unwrap_or_default();
            let output_tokens = usage.output_tokens.unwrap_or_default();
            let total_tokens = input_tokens + output_tokens;

            #[cfg(feature = "metrics")]
            emit_usage(
                model,
                input_tokens.into(),
                output_tokens.into(),
                total_tokens.into(),
                self.metric_metadata.as_ref(),
            );

            let usage = Usage {
                prompt_tokens: input_tokens,
                completion_tokens: output_tokens,
                total_tokens,
            };
            if let Some(callback) = &self.on_usage {
                callback(&usage).await?;
            }

            let usage = UsageBuilder::default()
                .prompt_tokens(input_tokens)
                .completion_tokens(output_tokens)
                .total_tokens(total_tokens)
                .build()
                .map_err(LanguageModelError::permanent)?;

            builder.usage(usage);
        }
        builder.build().map_err(LanguageModelError::from)
    }

    #[tracing::instrument(skip_all)]
    async fn complete_stream(&self, request: &ChatCompletionRequest) -> ChatCompletionStream {
        let model = &self.default_options.prompt_model;
        let request = match self
            .build_request(request)
            .and_then(|b| b.build().map_err(LanguageModelError::permanent))
        {
            Ok(request) => request,
            Err(e) => {
                return e.into();
            }
        };

        tracing::debug!(
            model = &model,
            messages = serde_json::to_string_pretty(&request).expect("Infallible"),
            "[ChatCompletion] Request to anthropic"
        );

        let response = self.client.messages().create_stream(request).await;

        let accumulating_response = Arc::new(Mutex::new(ChatCompletionResponse::default()));
        let final_response = Arc::clone(&accumulating_response);
        #[cfg(feature = "metrics")]
        let model = model.clone();
        #[cfg(feature = "metrics")]
        let metric_metadata = self.metric_metadata.clone();

        let maybe_usage_callback = self.on_usage.clone();

        response
            .map_ok(move |chunk| {
                let accumulating_response = Arc::clone(&accumulating_response);

                let mut lock = accumulating_response.lock().unwrap();

                append_delta_from_chunk(&chunk, &mut lock);
                lock.clone()
            })
            .map_err(LanguageModelError::permanent)
            .chain(
                stream::iter(vec![final_response]).map(move |final_response| {
                    if let Some(usage) = final_response.lock().unwrap().usage.as_ref() {
                        if let Some(callback) = maybe_usage_callback.as_ref() {
                            let usage = usage.clone();
                            let callback = callback.clone();

                            tokio::spawn(async move {
                                if let Err(e) = callback(&usage).await {
                                    tracing::error!("Error in on_usage callback: {}", e);
                                }
                            });
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

                    Ok(final_response.lock().unwrap().clone())
                }),
            )
            .boxed()
    }
}

#[allow(clippy::collapsible_match)]
fn append_delta_from_chunk(chunk: &MessagesStreamEvent, lock: &mut ChatCompletionResponse) {
    match chunk {
        MessagesStreamEvent::ContentBlockStart {
            index,
            content_block,
        } => match content_block {
            MessageContent::ToolUse(tool_use) => {
                lock.append_tool_call_delta(*index, Some(&tool_use.id), Some(&tool_use.name), None);
            }
            MessageContent::Text(text) => {
                lock.append_message_delta(Some(&text.text));
            }
            MessageContent::ToolResult(_tool_result) => (),
        },
        MessagesStreamEvent::ContentBlockDelta { index, delta } => match delta {
            async_anthropic::types::ContentBlockDelta::TextDelta { text } => {
                lock.append_message_delta(Some(text));
            }
            async_anthropic::types::ContentBlockDelta::InputJsonDelta { partial_json } => {
                lock.append_tool_call_delta(*index, None, None, Some(partial_json));
            }
        },
        #[allow(clippy::cast_possible_truncation)]
        MessagesStreamEvent::MessageDelta { usage, .. } => {
            if let Some(usage) = usage {
                let input_tokens = usage.input_tokens.unwrap_or_default();
                let output_tokens = usage.output_tokens.unwrap_or_default();
                let total_tokens = input_tokens + output_tokens;
                lock.append_usage_delta(input_tokens, output_tokens, total_tokens);
            }
        }

        MessagesStreamEvent::MessageStart { message, usage } => {
            if let Some(usage) = usage {
                let input_tokens = usage.input_tokens.unwrap_or_default();
                let output_tokens = usage.output_tokens.unwrap_or_default();
                let total_tokens = input_tokens + output_tokens;
                lock.append_usage_delta(input_tokens, output_tokens, total_tokens);
            }
            if let Some(message_usage) = &message.usage {
                let input_tokens = message_usage.input_tokens.unwrap_or_default();
                let output_tokens = message_usage.output_tokens.unwrap_or_default();
                let total_tokens = input_tokens + output_tokens;
                lock.append_usage_delta(input_tokens, output_tokens, total_tokens);
            }
        }
        _ => {}
    }
}

impl Anthropic {
    fn build_request(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<async_anthropic::types::CreateMessagesRequestBuilder, LanguageModelError> {
        let model = &self.default_options.prompt_model;
        let mut messages = request.messages().to_vec();

        let maybe_system = messages
            .iter()
            .position(ChatMessage::is_system)
            .map(|idx| messages.remove(idx));

        let messages = messages
            .iter()
            .map(message_to_antropic)
            .collect::<Result<Vec<_>>>()?;

        let mut anthropic_request = CreateMessagesRequestBuilder::default()
            .model(model)
            .messages(messages)
            .to_owned();

        if let Some(ChatMessage::System(system)) = maybe_system {
            anthropic_request.system(system);
        }

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

        Ok(anthropic_request)
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
                        .input(tool_call.args().and_then(|v| v.parse::<Value>().ok()))
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
    let mut properties = serde_json::Map::new();
    for param in &spec.parameters {
        properties.insert(
            param.name.to_string(),
            json!({

                        "type": param.ty.as_ref(),
                        "description": &param.description,
            }),
        );
    }
    let mut map = json!({
        "name": &spec.name,
        "description": &spec.description,
    })
    .as_object_mut()
    .context("Failed to build tool")?
    .to_owned();

    let required = spec
        .parameters
        .iter()
        .filter(|param| param.required)
        .map(|param| &param.name)
        .collect::<Vec<_>>();

    map.insert(
        "input_schema".to_string(),
        json!({
            "type": "object",
            "properties": properties,
            "required": required
        }),
    );

    Ok(map)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use swiftide_core::{
        AgentContext, Tool,
        chat_completion::{ChatCompletionRequest, ChatMessage, ParamSpec},
    };
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_partial_json, method, path},
    };

    #[derive(Clone)]
    struct FakeTool();

    #[async_trait]
    impl Tool for FakeTool {
        async fn invoke(
            &self,
            _agent_context: &dyn AgentContext,
            _tool_call: &ToolCall,
        ) -> std::result::Result<
            swiftide_core::chat_completion::ToolOutput,
            swiftide_core::chat_completion::errors::ToolError,
        > {
            todo!()
        }

        fn name(&self) -> std::borrow::Cow<'_, str> {
            "get_weather".into()
        }

        fn tool_spec(&self) -> ToolSpec {
            ToolSpec::builder()
                .description("Gets the weather")
                .name("get_weather")
                .parameters(vec![
                    ParamSpec::builder()
                        .description("Location")
                        .name("location")
                        .required(true)
                        .build()
                        .unwrap(),
                ])
                .build()
                .unwrap()
        }
    }

    #[test_log::test(tokio::test)]
    async fn test_complete_without_tools() {
        // Start a wiremock server
        let mock_server = MockServer::start().await;

        // Create a mock response
        let mock_response = ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{"type": "text", "text": "mocked response"}]
        }));

        // Mock the expected endpoint
        Mock::given(method("POST"))
            .and(path("/v1/messages")) // Adjust path to match expected endpoint
            .respond_with(mock_response)
            .mount(&mock_server)
            .await;

        let client = async_anthropic::Client::builder()
            .base_url(mock_server.uri())
            .build()
            .unwrap();

        // Build an Anthropic client with the mock server's URL
        let mut client_builder = Anthropic::builder();
        client_builder.client(client);
        let client = client_builder.build().unwrap();

        // Prepare a sample request
        let request = ChatCompletionRequest::builder()
            .messages(vec![ChatMessage::User("hello".into())])
            .build()
            .unwrap();

        // Call the complete method
        let result = client.complete(&request).await.unwrap();

        // Assert the result
        assert_eq!(result.message, Some("mocked response".into()));
        assert!(result.tool_calls.is_none());
    }

    #[test_log::test(tokio::test)]
    async fn test_complete_with_tools() {
        // Start a wiremock server
        let mock_server = MockServer::start().await;

        // Create a mock response
        let mock_response = ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_016zKNb88WhhgBQXhSaQf1rs",
            "content": [
            {
                "type": "text",
                "text": "I'll check the current weather in San Francisco, CA for you."
            },
            {
                "type": "tool_use",
                "id": "toolu_01E1yxpxXU4hBgCMLzPL1FuR",
                "input": {
                "location": "San Francisco, CA"
                },
                "name": "get_weather"
            }
            ],
            "model": "claude-3-5-sonnet-20241022",
            "stop_reason": "tool_use",
            "stop_sequence": null,
            "usage": {
            "input_tokens": 403,
            "output_tokens": 71
            }
        }));

        // Mock the expected endpoint
        Mock::given(method("POST"))
            .and(path("/v1/messages")) // Adjust path to match expected endpoint
            .respond_with(mock_response)
            .mount(&mock_server)
            .await;

        let client = async_anthropic::Client::builder()
            .base_url(mock_server.uri())
            .build()
            .unwrap();

        // Build an Anthropic client with the mock server's URL
        let mut client_builder = Anthropic::builder();
        client_builder.client(client);
        let client = client_builder.build().unwrap();

        // Prepare a sample request
        let request = ChatCompletionRequest::builder()
            .messages(vec![ChatMessage::User("hello".into())])
            .tools_spec(HashSet::from([FakeTool().tool_spec()]))
            .build()
            .unwrap();

        // Call the complete method
        let result = client.complete(&request).await.unwrap();

        // Assert the result
        assert_eq!(
            result.message,
            Some("I'll check the current weather in San Francisco, CA for you.".into())
        );
        assert!(result.tool_calls.is_some());

        let Some(tool_call) = result.tool_calls.and_then(|f| f.first().cloned()) else {
            panic!("No tool call found")
        };
        assert_eq!(tool_call.name(), "get_weather");
        assert_eq!(
            tool_call.args(),
            Some(
                json!({"location": "San Francisco, CA"})
                    .to_string()
                    .as_str()
            )
        );
    }

    #[test_log::test(tokio::test)]
    async fn test_complete_with_system_prompt() {
        // Start a wiremock server
        let mock_server = MockServer::start().await;

        // Create a mock response
        let mock_response = ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{"type": "text", "text": "Response with system prompt"}],
            "usage": {
                "input_tokens": 19,
                "output_tokens": 10,
            }
        }));

        // Mock the expected endpoint
        Mock::given(method("POST"))
            .and(path("/v1/messages")) // Adjust path to match expected endpoint
            .and(body_partial_json(json!({
                "system": "System message",
                "messages":[{"role":"user","content":[{"type":"text","text":"Hello"}]}]
            })))
            .respond_with(mock_response)
            .mount(&mock_server)
            .await;

        let client = async_anthropic::Client::builder()
            .base_url(mock_server.uri())
            .build()
            .unwrap();

        // Build an Anthropic client with the mock server's URL
        let mut client_builder = Anthropic::builder();
        client_builder.client(client);
        let client = client_builder.build().unwrap();

        // Prepare a sample request with a system message
        let request = ChatCompletionRequest::builder()
            .messages(vec![
                ChatMessage::System("System message".into()),
                ChatMessage::User("Hello".into()),
            ])
            .build()
            .unwrap();

        // Call the complete method
        let response = client.complete(&request).await.unwrap();

        // Assert the result
        assert_eq!(response.message, Some("Response with system prompt".into()));

        let usage = response.usage.unwrap();
        assert_eq!(usage.prompt_tokens, 19);
        assert_eq!(usage.completion_tokens, 10);
        assert_eq!(usage.total_tokens, 29);
    }

    #[test]
    fn test_tools_to_anthropic() {
        let tool_spec = ToolSpec::builder()
            .description("Gets the weather")
            .name("get_weather")
            .parameters(vec![
                ParamSpec::builder()
                    .description("Location")
                    .name("location")
                    .required(true)
                    .build()
                    .unwrap(),
            ])
            .build()
            .unwrap();

        let result = tools_to_anthropic(&tool_spec).unwrap();

        let expected = json!({
            "name": "get_weather",
            "description": "Gets the weather",
            "input_schema": {
                "type": "object",
                "properties": {
                    "location": {
                        "type": "string",
                        "description": "Location"
                    }
                },
                "required": ["location"]
            }
        });

        assert_eq!(result, expected.as_object().unwrap().to_owned());
    }
}
