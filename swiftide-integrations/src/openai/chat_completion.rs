use std::sync::Arc;
use std::sync::Mutex;

use anyhow::{Context as _, Result};
use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessageArgs,
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestToolMessageArgs,
    ChatCompletionRequestUserMessageArgs, ChatCompletionTool, ChatCompletionToolArgs,
    ChatCompletionToolType, FunctionCall, FunctionObjectArgs,
};
use async_trait::async_trait;
use futures_util::StreamExt as _;
use futures_util::stream;
use itertools::Itertools;
use serde_json::json;
use swiftide_core::ChatCompletionStream;
use swiftide_core::chat_completion::UsageBuilder;
use swiftide_core::chat_completion::{
    ChatCompletion, ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ToolCall, ToolSpec,
    errors::LanguageModelError,
};
#[cfg(feature = "metrics")]
use swiftide_core::metrics::emit_usage;

use super::GenericOpenAI;
use super::openai_error_to_language_model_error;

#[async_trait]
impl<
    C: async_openai::config::Config + std::default::Default + Sync + Send + std::fmt::Debug + Clone,
> ChatCompletion for GenericOpenAI<C>
{
    #[tracing::instrument(skip_all)]
    async fn complete(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, LanguageModelError> {
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
        let mut openai_request = self
            .chat_completion_request_defaults()
            .model(model)
            .messages(messages)
            .to_owned();

        if !request.tools_spec.is_empty() {
            openai_request
                .tools(
                    request
                        .tools_spec()
                        .iter()
                        .map(tools_to_openai)
                        .collect::<Result<Vec<_>>>()?,
                )
                .tool_choice("auto");
            if let Some(par) = self.default_options.parallel_tool_calls {
                openai_request.parallel_tool_calls(par);
            }
        }

        let request = openai_request
            .build()
            .map_err(openai_error_to_language_model_error)?;

        tracing::debug!(model, ?request, "Sending request to OpenAI");

        let response = self
            .client
            .chat()
            .create(request)
            .await
            .map_err(openai_error_to_language_model_error)?;

        tracing::debug!(?response, "Received response from OpenAI");

        let mut builder = ChatCompletionResponse::builder()
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
            .to_owned();

        if let Some(usage) = &response.usage {
            let usage = UsageBuilder::default()
                .prompt_tokens(usage.prompt_tokens)
                .completion_tokens(usage.completion_tokens)
                .total_tokens(usage.total_tokens)
                .build()
                .map_err(LanguageModelError::permanent)?;

            builder.usage(usage);

            #[cfg(feature = "metrics")]
            {
                if let Some(usage) = response.usage.as_ref() {
                    emit_usage(
                        model,
                        usage.prompt_tokens.into(),
                        usage.completion_tokens.into(),
                        usage.total_tokens.into(),
                        self.metric_metadata.as_ref(),
                    );
                }
            }
        }

        builder.build().map_err(LanguageModelError::from)
    }

    #[tracing::instrument(skip_all)]
    async fn complete_stream(&self, request: &ChatCompletionRequest) -> ChatCompletionStream {
        let Some(model) = self.default_options.prompt_model.as_ref() else {
            return LanguageModelError::permanent("Model not set").into();
        };

        let messages = match request
            .messages()
            .iter()
            .map(message_to_openai)
            .collect::<Result<Vec<_>>>()
        {
            Ok(messages) => messages,
            Err(e) => return LanguageModelError::from(e).into(),
        };

        // Build the request to be sent to the OpenAI API.
        let mut openai_request = self
            .chat_completion_request_defaults()
            .model(model)
            .messages(messages)
            .to_owned();

        if !request.tools_spec.is_empty() {
            openai_request
                .tools(
                    match request
                        .tools_spec()
                        .iter()
                        .map(tools_to_openai)
                        .collect::<Result<Vec<_>>>()
                    {
                        Ok(tools) => tools,
                        Err(e) => {
                            return LanguageModelError::from(e).into();
                        }
                    },
                )
                .tool_choice("auto");
            if let Some(par) = self.default_options.parallel_tool_calls {
                openai_request.parallel_tool_calls(par);
            }
        }

        let request = match openai_request.build() {
            Ok(request) => request,
            Err(e) => {
                return openai_error_to_language_model_error(e).into();
            }
        };

        tracing::debug!(model, ?request, "Sending request to OpenAI");

        let response = match self.client.chat().create_stream(request).await {
            Ok(response) => response,
            Err(e) => return openai_error_to_language_model_error(e).into(),
        };

        let accumulating_response = Arc::new(Mutex::new(ChatCompletionResponse::default()));
        let final_response = accumulating_response.clone();
        let stream_full = self.stream_full;

        #[cfg(feature = "metrics")]
        let metric_metadata = self.metric_metadata.clone();
        #[cfg(feature = "metrics")]
        let model = model.to_string();

        response
            .map(move |chunk| match chunk {
                Ok(chunk) => {
                    let accumulating_response = Arc::clone(&accumulating_response);

                    let delta_message = chunk.choices[0].delta.content.as_deref();
                    let delta_tool_calls = chunk.choices[0].delta.tool_calls.as_deref();
                    let usage = chunk.usage.as_ref();

                    let chat_completion_response = {
                        let mut lock = accumulating_response.lock().unwrap();
                        lock.append_message_delta(delta_message);

                        if let Some(delta_tool_calls) = delta_tool_calls {
                            for tc in delta_tool_calls {
                                lock.append_tool_call_delta(
                                    tc.index as usize,
                                    tc.id.as_deref(),
                                    tc.function.as_ref().and_then(|f| f.name.as_deref()),
                                    tc.function.as_ref().and_then(|f| f.arguments.as_deref()),
                                );
                            }
                        }

                        if let Some(usage) = usage {
                            lock.append_usage_delta(
                                usage.prompt_tokens,
                                usage.completion_tokens,
                                usage.total_tokens,
                            );
                        }

                        if stream_full {
                            lock.clone()
                        } else {
                            // If we are not streaming the full response, we return a clone of the
                            // current state to avoid holding the lock
                            // for too long.
                            ChatCompletionResponse {
                                id: lock.id,
                                message: None,
                                tool_calls: None,
                                usage: None,
                                delta: lock.delta.clone(),
                            }
                        }
                    };

                    Ok(chat_completion_response)
                }
                Err(e) => Err(openai_error_to_language_model_error(e)),
            })
            .chain(
                stream::iter(vec![final_response]).map(move |accumulating_response| {
                    let lock = accumulating_response.lock().unwrap();

                    #[cfg(feature = "metrics")]
                    {
                        if let Some(usage) = lock.usage.as_ref() {
                            emit_usage(
                                &model,
                                usage.prompt_tokens.into(),
                                usage.completion_tokens.into(),
                                usage.total_tokens.into(),
                                metric_metadata.as_ref(),
                            );
                        }
                    }
                    Ok(lock.clone())
                }),
            )
            .boxed()
    }
}

fn tools_to_openai(spec: &ToolSpec) -> Result<ChatCompletionTool> {
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

    ChatCompletionToolArgs::default()
        .r#type(ChatCompletionToolType::Function)
        .function(FunctionObjectArgs::default()
            .name(&spec.name)
            .description(&spec.description)
            .strict(true)
            .parameters(json!({
                "type": "object",
                "properties": properties,
                "required": spec.parameters.iter().filter(|param| param.required).map(|param| &param.name).collect_vec(),
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
        ChatMessage::Summary(msg) => ChatCompletionRequestAssistantMessageArgs::default()
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
                        .map(|tool_call| ChatCompletionMessageToolCall {
                            id: tool_call.id().to_string(),
                            r#type: ChatCompletionToolType::Function,
                            function: FunctionCall {
                                name: tool_call.name().to_string(),
                                arguments: tool_call.args().unwrap_or_default().to_string(),
                            },
                        })
                        .collect::<Vec<_>>(),
                );
            }

            builder.build()?.into()
        }
    };

    Ok(openai_message)
}

#[cfg(test)]
mod tests {
    use crate::openai::{OpenAI, Options};

    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test_log::test(tokio::test)]
    async fn test_complete() {
        let mock_server = MockServer::start().await;

        // Mock OpenAI API response
        let response_body = json!({
          "id": "chatcmpl-B9MBs8CjcvOU2jLn4n570S5qMJKcT",
          "object": "chat.completion",
          "created": 123,
          "model": "gpt-4o",
          "choices": [
            {
              "index": 0,
              "message": {
                "role": "assistant",
                "content": "Hello, world!",
                "refusal": null,
                "annotations": []
              },
              "logprobs": null,
              "finish_reason": "stop"
            }
          ],
          "usage": {
            "prompt_tokens": 19,
            "completion_tokens": 10,
            "total_tokens": 29,
            "prompt_tokens_details": {
              "cached_tokens": 0,
              "audio_tokens": 0
            },
            "completion_tokens_details": {
              "reasoning_tokens": 0,
              "audio_tokens": 0,
              "accepted_prediction_tokens": 0,
              "rejected_prediction_tokens": 0
            }
          },
          "service_tier": "default"
        });
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
            .mount(&mock_server)
            .await;

        // Create a GenericOpenAI instance with the mock server URL
        let config = async_openai::config::OpenAIConfig::new().with_api_base(mock_server.uri());
        let async_openai = async_openai::Client::with_config(config);

        let openai = OpenAI::builder()
            .client(async_openai)
            .default_prompt_model("gpt-4o")
            .build()
            .expect("Can create OpenAI client.");

        // Prepare a test request
        let request = ChatCompletionRequest::builder()
            .messages(vec![ChatMessage::User("Hi".to_string())])
            .build()
            .unwrap();

        // Call the `complete` method
        let response = openai.complete(&request).await.unwrap();

        // Assert the response
        assert_eq!(response.message(), Some("Hello, world!"));

        // Usage
        let usage = response.usage.unwrap();
        assert_eq!(usage.prompt_tokens, 19);
        assert_eq!(usage.completion_tokens, 10);
        assert_eq!(usage.total_tokens, 29);
    }

    #[test_log::test(tokio::test)]
    #[allow(clippy::items_after_statements)]
    async fn test_complete_with_all_default_settings() {
        use serde_json::Value;
        use wiremock::{Request, Respond, ResponseTemplate};

        let mock_server = wiremock::MockServer::start().await;

        // Custom matcher to validate all settings in the incoming request
        struct ValidateAllSettings;

        impl Respond for ValidateAllSettings {
            fn respond(&self, request: &Request) -> ResponseTemplate {
                let v: Value = serde_json::from_slice(&request.body).unwrap();

                // Validate required fields
                assert_eq!(v["model"], "gpt-4-turbo");
                let arr = v["messages"].as_array().unwrap();
                assert_eq!(arr.len(), 1);
                assert_eq!(arr[0]["content"], "Test");

                assert_eq!(v["parallel_tool_calls"], true);
                assert_eq!(v["max_completion_tokens"], 77);
                assert!((v["temperature"].as_f64().unwrap() - 0.42).abs() < 1e-5);
                assert_eq!(v["reasoning_effort"], "low");
                assert_eq!(v["seed"], 42);
                assert!((v["presence_penalty"].as_f64().unwrap() - 1.1).abs() < 1e-5);

                // Metadata as JSON object and user string
                assert_eq!(v["metadata"], serde_json::json!({"key": "value"}));
                assert_eq!(v["user"], "test-user");
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "chatcmpl-xxx",
                "object": "chat.completion",
                "created": 123,
                "model": "gpt-4-turbo",
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "All settings validated",
                        "refusal": null,
                        "annotations": []
                    },
                    "logprobs": null,
                    "finish_reason": "stop"
                }],
                "usage": {
                    "prompt_tokens": 19,
                    "completion_tokens": 10,
                    "total_tokens": 29,
                    "prompt_tokens_details": {"cached_tokens": 0, "audio_tokens": 0},
                    "completion_tokens_details": {"reasoning_tokens": 0, "audio_tokens": 0, "accepted_prediction_tokens": 0, "rejected_prediction_tokens": 0}
                },
                "service_tier": "default"
            }))
            }
        }

        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/chat/completions"))
            .respond_with(ValidateAllSettings)
            .mount(&mock_server)
            .await;

        let config = async_openai::config::OpenAIConfig::new().with_api_base(mock_server.uri());
        let async_openai = async_openai::Client::with_config(config);

        let openai = crate::openai::OpenAI::builder()
            .client(async_openai)
            .default_prompt_model("gpt-4-turbo")
            .default_embed_model("not-used")
            .parallel_tool_calls(Some(true))
            .default_options(
                Options::builder()
                    .max_completion_tokens(77)
                    .temperature(0.42)
                    .reasoning_effort(async_openai::types::ReasoningEffort::Low)
                    .seed(42)
                    .presence_penalty(1.1)
                    .metadata(serde_json::json!({"key": "value"}))
                    .user("test-user"),
            )
            .build()
            .expect("Can create OpenAI client.");

        let request = swiftide_core::chat_completion::ChatCompletionRequest::builder()
            .messages(vec![swiftide_core::chat_completion::ChatMessage::User(
                "Test".to_string(),
            )])
            .build()
            .unwrap();

        let response = openai.complete(&request).await.unwrap();

        assert_eq!(response.message(), Some("All settings validated"));
    }
}
