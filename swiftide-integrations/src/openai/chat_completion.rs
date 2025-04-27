use std::sync::Arc;
use std::sync::Mutex;

use anyhow::{Context as _, Result};
use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessageArgs,
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestToolMessageArgs,
    ChatCompletionRequestUserMessageArgs, ChatCompletionTool, ChatCompletionToolArgs,
    ChatCompletionToolType, CreateChatCompletionRequestArgs, FunctionCall, FunctionObjectArgs,
};
use async_trait::async_trait;
use futures_util::StreamExt as _;
use itertools::Itertools;
use serde_json::json;
use swiftide_core::ChatCompletionStream;
use swiftide_core::chat_completion::{
    ChatCompletion, ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ToolCall, ToolSpec,
    errors::LanguageModelError,
};

use super::GenericOpenAI;
use super::openai_error_to_language_model_error;

#[async_trait]
impl<C: async_openai::config::Config + std::default::Default + Sync + Send + std::fmt::Debug>
    ChatCompletion for GenericOpenAI<C>
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
        let mut openai_request = CreateChatCompletionRequestArgs::default()
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

        tracing::debug!(
            model = &model,
            request = serde_json::to_string_pretty(&request).expect("infallible"),
            "Sending request to OpenAI"
        );

        let response = self
            .client
            .chat()
            .create(request)
            .await
            .map_err(openai_error_to_language_model_error)?;

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
            .map_err(LanguageModelError::from)
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
        let mut openai_request = CreateChatCompletionRequestArgs::default()
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

        tracing::debug!(
            model = &model,
            request = serde_json::to_string_pretty(&request).expect("infallible"),
            "Sending request to OpenAI"
        );

        let response = match self.client.chat().create_stream(request).await {
            Ok(response) => response,
            Err(e) => return openai_error_to_language_model_error(e).into(),
        };

        let accumulating_response = Arc::new(Mutex::new(ChatCompletionResponse::default()));

        response
            .map(move |chunk| match chunk {
                Ok(chunk) => {
                    let accumulating_response = Arc::clone(&accumulating_response);

                    let delta_message = chunk.choices[0].delta.content.as_deref();
                    let delta_tool_calls = chunk.choices[0].delta.tool_calls.as_deref();

                    let chat_completion_response = {
                        let mut lock = accumulating_response.lock().unwrap();
                        lock.append_message_delta(delta_message);

                        if let Some(delta_tool_calls) = delta_tool_calls {
                            for tc in delta_tool_calls {
                                lock.append_tool_call_delta(
                                    tc.index,
                                    tc.id.as_deref(),
                                    tc.function.as_ref().and_then(|f| f.name.as_deref()),
                                    tc.function.as_ref().and_then(|f| f.arguments.as_deref()),
                                );
                            }
                        }

                        lock.clone()
                    };

                    Ok(chat_completion_response)
                }
                Err(e) => Err(openai_error_to_language_model_error(e)),
            })
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
    use crate::openai::OpenAI;

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
    }
}
