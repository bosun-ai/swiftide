use std::sync::Arc;
use std::sync::Mutex;

use anyhow::{Context as _, Result};
use async_openai::error::OpenAIError;
use async_openai::types::ChatCompletionStreamOptions;
use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessageArgs,
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestToolMessageArgs,
    ChatCompletionRequestUserMessageArgs, ChatCompletionTool, ChatCompletionToolArgs,
    ChatCompletionToolType, FunctionCall, FunctionObjectArgs,
};
use async_trait::async_trait;
use futures_util::StreamExt as _;
use futures_util::future;
use futures_util::stream;
use itertools::Itertools;
use serde::Serialize;
use serde_json::json;
use swiftide_core::ChatCompletionStream;
use swiftide_core::chat_completion::{
    ChatCompletion, ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ToolCall, ToolSpec,
    errors::LanguageModelError,
};
use swiftide_core::chat_completion::{Usage, UsageBuilder};
#[cfg(feature = "metrics")]
use swiftide_core::metrics::emit_usage;

use super::GenericOpenAI;
use super::openai_error_to_language_model_error;
use super::responses_api::{
    ResponsesStreamAccumulator, StreamChunk, StreamControl, build_responses_request_from_chat,
    response_to_chat_completion,
};
use tracing_futures::Instrument;

#[async_trait]
impl<
    C: async_openai::config::Config
        + std::default::Default
        + Sync
        + Send
        + std::fmt::Debug
        + Clone
        + 'static,
> ChatCompletion for GenericOpenAI<C>
{
    #[cfg_attr(not(feature = "langfuse"), tracing::instrument(skip_all, err))]
    #[cfg_attr(
        feature = "langfuse",
        tracing::instrument(skip_all, err, fields(langfuse.type = "GENERATION"))
    )]
    async fn complete(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, LanguageModelError> {
        if self.is_responses_api_enabled() {
            return self.complete_via_responses_api(request).await;
        }

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

        tracing::trace!(model, ?request, "Sending request to OpenAI");

        let response = self
            .client
            .chat()
            .create(request.clone())
            .await
            .map_err(openai_error_to_language_model_error)?;

        tracing::trace!(?response, "[ChatCompletion] Full response from OpenAI");
        // Make sure the debug log is a concise one line

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
        }

        let our_response = builder.build().map_err(LanguageModelError::from)?;

        self.track_completion(
            model,
            our_response.usage.as_ref(),
            Some(&request),
            Some(&our_response),
        );

        Ok(our_response)
    }

    #[tracing::instrument(skip_all)]
    async fn complete_stream(&self, request: &ChatCompletionRequest) -> ChatCompletionStream {
        if self.is_responses_api_enabled() {
            return self.complete_stream_via_responses_api(request).await;
        }

        let Some(model_name) = self.default_options.prompt_model.clone() else {
            return LanguageModelError::permanent("Model not set").into();
        };

        #[cfg(not(any(feature = "metrics", feature = "langfuse")))]
        let _ = &model_name;

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
            .model(&model_name)
            .messages(messages)
            .stream_options(ChatCompletionStreamOptions {
                include_usage: true,
            })
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

        tracing::trace!(model = %model_name, ?request, "Sending request to OpenAI");

        let response = match self.client.chat().create_stream(request.clone()).await {
            Ok(response) => response,
            Err(e) => return openai_error_to_language_model_error(e).into(),
        };

        let accumulating_response = Arc::new(Mutex::new(ChatCompletionResponse::default()));
        let final_response = accumulating_response.clone();
        let stream_full = self.stream_full;

        let span = if cfg!(feature = "langfuse") {
            tracing::info_span!(
                "stream",
                langfuse.type = "GENERATION",
            )
        } else {
            tracing::info_span!("stream")
        };

        let self_for_stream = self.clone();
        let stream = response
            .map(move |chunk| match chunk {
                Ok(chunk) => {
                    let accumulating_response = Arc::clone(&accumulating_response);

                    let delta_message = chunk
                        .choices
                        .first()
                        .and_then(|d| d.delta.content.as_deref());
                    let delta_tool_calls = chunk
                        .choices
                        .first()
                        .and_then(|d| d.delta.tool_calls.as_deref());
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

                    self_for_stream.track_completion(
                        &model_name,
                        lock.usage.as_ref(),
                        Some(&request),
                        Some(&*lock),
                    );

                    Ok(lock.clone())
                }),
            );

        let stream = tracing_futures::Instrument::instrument(stream, span);

        Box::pin(stream)
    }
}

impl<
    C: async_openai::config::Config
        + std::default::Default
        + Sync
        + Send
        + std::fmt::Debug
        + Clone
        + 'static,
> GenericOpenAI<C>
{
    async fn complete_via_responses_api(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, LanguageModelError> {
        let model = self
            .default_options
            .prompt_model
            .as_ref()
            .context("Model not set")?;

        let create_request = build_responses_request_from_chat(self, request)?;

        let response = self
            .client
            .responses()
            .create(create_request.clone())
            .await
            .map_err(openai_error_to_language_model_error)?;

        let completion = response_to_chat_completion(&response)?;

        self.track_completion(
            model,
            completion.usage.as_ref(),
            Some(&create_request),
            Some(&completion),
        );

        Ok(completion)
    }

    #[allow(clippy::too_many_lines)]
    async fn complete_stream_via_responses_api(
        &self,
        request: &ChatCompletionRequest,
    ) -> ChatCompletionStream {
        #[allow(unused_variables)]
        let Some(model_name) = self.default_options.prompt_model.clone() else {
            return LanguageModelError::permanent("Model not set").into();
        };

        let mut create_request = match build_responses_request_from_chat(self, request) {
            Ok(req) => req,
            Err(err) => return err.into(),
        };

        create_request.stream = Some(true);

        let stream = match self
            .client
            .responses()
            .create_stream(create_request.clone())
            .await
        {
            Ok(stream) => stream,
            Err(err) => return openai_error_to_language_model_error(err).into(),
        };

        let aggregator = Arc::new(Mutex::new(ResponsesStreamAccumulator::new()));
        let stream_full = self.stream_full;

        let span = if cfg!(feature = "langfuse") {
            tracing::info_span!("responses_stream", langfuse.type = "GENERATION")
        } else {
            tracing::info_span!("responses_stream")
        };

        let mapped_stream = stream
            .then(move |event| {
                let aggregator = aggregator.clone();
                async move {
                    match event {
                        Ok(event) => {
                            let mut guard = aggregator.lock().expect("mutex poisoned");
                            match guard.apply_event(event, stream_full) {
                                Ok(StreamControl::Emit(chunk)) => Some(Ok((chunk, false))),
                                Ok(StreamControl::Finished(chunk)) => Some(Ok((chunk, true))),
                                Ok(StreamControl::Skip) => None,
                                Err(err) => Some(Err(err)),
                            }
                        }
                        Err(err) => {
                            if is_responses_stream_end_error(&err) {
                                let mut guard = aggregator.lock().expect("mutex poisoned");

                                if guard.has_emitted_finished() {
                                    None
                                } else {
                                    let chunk = guard.snapshot(stream_full, true);
                                    Some(Ok((chunk, true)))
                                }
                            } else {
                                Some(Err(openai_error_to_language_model_error(err)))
                            }
                        }
                    }
                }
            })
            .filter_map(|maybe| async move { maybe });

        let mapped_stream = mapped_stream.scan(false, |finished, result| {
            if *finished {
                return future::ready(None);
            }

            if result
                .as_ref()
                .map(|(_, finished)| *finished)
                .unwrap_or(true)
            {
                *finished = true;
            }

            future::ready(Some(result))
        });

        let this = self.clone();
        let mapped_stream = mapped_stream.map(
            move |result: Result<(StreamChunk, bool), LanguageModelError>| {
                result.map(|(chunk, finished)| {
                    let response = chunk.response;

                    if finished {
                        this.track_completion(
                            &model_name,
                            response.usage.as_ref(),
                            Some(&create_request),
                            Some(&response),
                        );
                    }

                    response
                })
            },
        );

        Box::pin(Instrument::instrument(mapped_stream, span))
    }
    #[allow(unused_variables)]
    pub(crate) fn track_completion<R, S>(
        &self,
        model: &str,
        usage: Option<&Usage>,
        request: Option<&R>,
        response: Option<&S>,
    ) where
        R: Serialize + ?Sized,
        S: Serialize + ?Sized,
    {
        if let Some(usage) = usage {
            let cb_usage = usage.clone();
            if let Some(callback) = &self.on_usage {
                let callback = callback.clone();
                tokio::spawn(async move {
                    if let Err(err) = callback(&cb_usage).await {
                        tracing::error!("Error in on_usage callback: {err}");
                    }
                });
            }

            #[cfg(feature = "metrics")]
            emit_usage(
                model,
                usage.prompt_tokens.into(),
                usage.completion_tokens.into(),
                usage.total_tokens.into(),
                self.metric_metadata.as_ref(),
            );
        }

        #[cfg(feature = "langfuse")]
        tracing::debug!(
            langfuse.model = model,
            langfuse.input = request.and_then(langfuse_json).unwrap_or_default(),
            langfuse.output = response.and_then(langfuse_json).unwrap_or_default(),
            langfuse.usage = usage.and_then(langfuse_json).unwrap_or_default(),
        );
    }
}

fn is_responses_stream_end_error(error: &OpenAIError) -> bool {
    match error {
        OpenAIError::StreamError(message) => {
            let normalized = message.trim().to_ascii_lowercase();
            normalized == "stream ended" || normalized.contains("stream ended")
        }
        _ => false,
    }
}

#[cfg(feature = "langfuse")]
pub(crate) fn langfuse_json<T: Serialize + ?Sized>(value: &T) -> Option<String> {
    serde_json::to_string_pretty(value).ok()
}

#[cfg(not(feature = "langfuse"))]
#[allow(dead_code)]
pub(crate) fn langfuse_json<T>(_value: &T) -> Option<String> {
    None
}

pub(crate) fn usage_from_counts(
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
) -> Usage {
    Usage {
        prompt_tokens,
        completion_tokens,
        total_tokens,
    }
}

fn tools_to_openai(spec: &ToolSpec) -> Result<ChatCompletionTool> {
    let mut parameters = match &spec.parameters_schema {
        Some(schema) => serde_json::to_value(schema)?,
        None => json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false,
        }),
    };

    if let Some(obj) = parameters.as_object_mut() {
        obj.insert("additionalProperties".to_string(), json!(false));
        #[allow(clippy::collapsible_if)]
        if let Some(props) = obj.get_mut("properties").and_then(|v| v.as_object_mut()) {
            if props.get("type").map(serde_json::Value::as_str) == Some(Some("object")) {
                props.insert("additionalProperties".to_string(), json!(false));
            }
        }
    } else {
        return Err(anyhow::anyhow!(
            "Tool parameters schema is not a JSON object"
        ));
    }
    tracing::debug!(
        parameters = serde_json::to_string_pretty(&parameters).unwrap(),
        tool = %spec.name,
        "Tool parameters schema"
    );

    ChatCompletionToolArgs::default()
        .r#type(ChatCompletionToolType::Function)
        .function(
            FunctionObjectArgs::default()
                .name(&spec.name)
                .description(&spec.description)
                .strict(true)
                .parameters(parameters)
                .build()?,
        )
        .build()
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
    async fn test_complete_responses_api() {
        use serde_json::Value;
        use wiremock::{Request, Respond};

        let mock_server = MockServer::start().await;

        use async_openai::types::responses::{
            CompletionTokensDetails, Content, OutputContent, OutputMessage, OutputStatus,
            OutputText, PromptTokensDetails, Response as ResponsesResponse, Role, Status,
            Usage as ResponsesUsage,
        };

        let response = ResponsesResponse {
            created_at: 123,
            error: None,
            id: "resp_123".into(),
            incomplete_details: None,
            instructions: None,
            max_output_tokens: None,
            metadata: None,
            model: "gpt-4.1-mini".into(),
            object: "response".into(),
            output: vec![OutputContent::Message(OutputMessage {
                content: vec![Content::OutputText(OutputText {
                    annotations: Vec::new(),
                    text: "Hello via responses".into(),
                })],
                id: "msg_1".into(),
                role: Role::Assistant,
                status: OutputStatus::Completed,
            })],
            output_text: Some("Hello via responses".into()),
            parallel_tool_calls: None,
            previous_response_id: None,
            reasoning: None,
            store: None,
            service_tier: None,
            status: Status::Completed,
            temperature: None,
            text: None,
            tool_choice: None,
            tools: None,
            top_p: None,
            truncation: None,
            usage: Some(ResponsesUsage {
                input_tokens: 5,
                input_tokens_details: PromptTokensDetails {
                    audio_tokens: Some(0),
                    cached_tokens: Some(0),
                },
                output_tokens: 3,
                output_tokens_details: CompletionTokensDetails {
                    accepted_prediction_tokens: Some(0),
                    audio_tokens: Some(0),
                    reasoning_tokens: Some(0),
                    rejected_prediction_tokens: Some(0),
                },
                total_tokens: 8,
            }),
            user: None,
        };

        let response_body = serde_json::to_value(&response).unwrap();

        struct ValidateResponsesRequest {
            expected_model: &'static str,
            response: Value,
        }

        impl Respond for ValidateResponsesRequest {
            fn respond(&self, request: &Request) -> ResponseTemplate {
                let body: Value = serde_json::from_slice(&request.body).unwrap();
                assert_eq!(body["model"], self.expected_model);
                let input = body["input"].as_array().expect("input array");
                assert_eq!(input.len(), 1);
                assert_eq!(input[0]["role"], "user");
                assert_eq!(input[0]["content"], "Hello via prompt");

                let _: async_openai::types::responses::Response =
                    serde_json::from_value(self.response.clone()).unwrap();

                ResponseTemplate::new(200).set_body_json(self.response.clone())
            }
        }

        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ValidateResponsesRequest {
                expected_model: "gpt-4.1-mini",
                response: response_body,
            })
            .mount(&mock_server)
            .await;

        let config = async_openai::config::OpenAIConfig::new().with_api_base(mock_server.uri());
        let async_openai = async_openai::Client::with_config(config);

        let openai = OpenAI::builder()
            .client(async_openai)
            .default_prompt_model("gpt-4.1-mini")
            .use_responses_api(true)
            .build()
            .expect("Can create OpenAI client.");

        let request = ChatCompletionRequest::builder()
            .messages(vec![ChatMessage::User("Hello via prompt".to_string())])
            .build()
            .unwrap();

        let response = openai.complete(&request).await.unwrap();

        assert_eq!(response.message(), Some("Hello via responses"));

        let usage = response.usage.expect("usage present");
        assert_eq!(usage.prompt_tokens, 5);
        assert_eq!(usage.completion_tokens, 3);
        assert_eq!(usage.total_tokens, 8);
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

    #[test]
    fn test_harmless_stream_end_detection() {
        assert!(is_responses_stream_end_error(&OpenAIError::StreamError(
            "Stream Ended".into()
        )));

        assert!(is_responses_stream_end_error(&OpenAIError::StreamError(
            "connection closed: stream ended".into()
        )));

        assert!(!is_responses_stream_end_error(&OpenAIError::StreamError(
            "Too Many Requests".into()
        )));
    }
}
