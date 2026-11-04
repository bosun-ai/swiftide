use super::error::mistral_error;
use super::types::{CompletionData, DeltaData};
use super::{Mistral, wire};
use async_openai::error::OpenAIError;
use async_openai::types::chat::{
    ChatCompletionRequestMessage, ChatCompletionStreamOptions, ChatCompletionToolChoiceOption,
    ChatCompletionTools, CompletionUsage, FunctionObject, ToolChoiceOptions,
};
use async_openai::types::responses::{ResponseFormat, ResponseFormatJsonSchema};
use async_trait::async_trait;
use futures_util::{StreamExt as _, stream};
use schemars::Schema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use swiftide_core::ChatCompletionStream;
use swiftide_core::chat_completion::{
    ChatCompletion, ChatCompletionRequest, ChatCompletionResponse, ToolSpec, Usage,
    errors::LanguageModelError,
};
#[cfg(feature = "metrics")]
use swiftide_core::metrics::emit_usage;
use tracing_futures::Instrument;

#[derive(Deserialize)]
struct ChatCompletionUsage {
    usage: Option<CompletionUsage>,
}

#[async_trait]
impl ChatCompletion for Mistral {
    #[cfg_attr(not(feature = "langfuse"), tracing::instrument(skip_all, err))]
    #[cfg_attr(feature = "langfuse", tracing::instrument(skip_all, err, fields(langfuse.type = "GENERATION")))]
    async fn complete(
        &self,
        request: &ChatCompletionRequest<'_>,
    ) -> Result<ChatCompletionResponse, LanguageModelError> {
        self.complete_with_schema(request, None).await
    }
    #[tracing::instrument(skip_all)]
    async fn complete_stream(&self, request: &ChatCompletionRequest<'_>) -> ChatCompletionStream {
        let stream = if self.use_responses_api {
            self.stream_responses(request).await
        } else {
            self.stream_chat(request).await
        };
        match stream {
            Ok(stream) => stream,
            Err(error) => error.into(),
        }
    }
}

impl Mistral {
    fn chat_request(
        &self,
        request: &ChatCompletionRequest<'_>,
        streaming: bool,
        schema: Option<Schema>,
    ) -> Result<Value, LanguageModelError> {
        let model = self
            .default_options
            .prompt_model
            .as_ref()
            .ok_or_else(|| LanguageModelError::permanent("Model not set"))?;
        let mut builder = self.request_defaults();
        // Encode native history, including Mistral thinking blocks.
        builder
            .model(model)
            .messages(Vec::<ChatCompletionRequestMessage>::new());
        if streaming {
            builder
                .stream(true)
                .stream_options(ChatCompletionStreamOptions {
                    include_usage: Some(true),
                    include_obfuscation: None,
                });
        }
        if !request.tools_spec().is_empty() {
            builder
                .tools(
                    request
                        .tools_spec()
                        .iter()
                        .map(tools_to_openai)
                        .collect::<Result<Vec<_>, _>>()?,
                )
                .tool_choice(ChatCompletionToolChoiceOption::Mode(
                    ToolChoiceOptions::Auto,
                ));
        }
        let defaults = builder.build().map_err(mistral_error)?;
        let mut body = wire::request_body(defaults, request.messages(), self.chat_format)?;
        body.as_object_mut()
            .expect("request object")
            .extend(self.default_options.extra_body.clone());
        if let Some(schema) = schema {
            // The explicitly requested schema takes precedence over default extra-body options.
            body["response_format"] = serde_json::to_value(ResponseFormat::JsonSchema {
                json_schema: ResponseFormatJsonSchema {
                    description: None,
                    name: "structured_prompt".into(),
                    schema: schema.into(),
                    strict: Some(true),
                },
            })
            .map_err(LanguageModelError::permanent)?;
        }
        Ok(body)
    }

    pub(super) async fn complete_with_schema(
        &self,
        request: &ChatCompletionRequest<'_>,
        schema: Option<Schema>,
    ) -> Result<ChatCompletionResponse, LanguageModelError> {
        if self.use_responses_api {
            return self.complete_responses(request, schema).await;
        }
        let body = self.chat_request(request, false, schema)?;
        let model = self
            .default_options
            .prompt_model
            .as_deref()
            .expect("validated model");
        tracing::trace!(model, request = ?body, "Sending chat completion request");
        let completion: wire::Completion =
            self.client
                .chat()
                .create_byot(&body)
                .await
                .map_err(|error| {
                    self.track_failed_chat_completion(model, &error, &body, None);
                    mistral_error(error)
                })?;
        let response = ChatCompletionResponse::from(Into::<CompletionData>::into(completion));
        tracing::trace!(?response, "Received chat completion");
        self.track_completion(model, response.usage.as_ref(), Some(&body), Some(&response));
        Ok(response)
    }

    pub(super) async fn stream_chat(
        &self,
        request: &ChatCompletionRequest<'_>,
    ) -> Result<ChatCompletionStream, LanguageModelError> {
        let body = self.chat_request(request, true, None)?;
        let model = self
            .default_options
            .prompt_model
            .clone()
            .expect("validated model");
        tracing::trace!(model, request = ?body, "Sending streaming chat completion request");
        let source = self
            .client
            .chat()
            .create_stream_byot::<_, wire::Chunk>(&body)
            .await
            .map_err(|error| {
                self.track_failed_chat_completion(&model, &error, &body, None);
                mistral_error(error)
            })?;
        let client = self.clone();
        let body = Arc::new(body);
        let span = if cfg!(feature = "langfuse") {
            tracing::info_span!("stream", langfuse.type = "GENERATION")
        } else {
            tracing::info_span!("stream")
        };
        let stream = stream::unfold(
            (source, ChatCompletionResponse::default(), false),
            move |(mut source, mut state, finished)| {
                let client = client.clone();
                let body = body.clone();
                let model = model.clone();
                async move {
                    if finished {
                        return None;
                    }
                    match source.next().await {
                        Some(Ok(chunk)) => {
                            let reasoning = Into::<DeltaData>::into(chunk).apply(&mut state);
                            let snapshot = if client.stream_full {
                                state.clone()
                            } else {
                                ChatCompletionResponse {
                                    id: state.id,
                                    delta: state.delta.clone(),
                                    reasoning,
                                    ..Default::default()
                                }
                            };
                            Some((Ok(snapshot), (source, state, false)))
                        }
                        Some(Err(error)) => {
                            client.track_failed_chat_completion(
                                &model,
                                &error,
                                body.as_ref(),
                                state.usage.as_ref(),
                            );
                            Some((Err(mistral_error(error)), (source, state, true)))
                        }
                        None => {
                            state.delta = None;
                            client.track_completion(
                                &model,
                                state.usage.as_ref(),
                                Some(body.as_ref()),
                                Some(&state),
                            );
                            Some((Ok(state.clone()), (source, state, true)))
                        }
                    }
                }
            },
        );
        Ok(Box::pin(stream.instrument(span)))
    }

    pub(super) fn track_failed_chat_completion(
        &self,
        model: &str,
        error: &OpenAIError,
        request: &impl Serialize,
        previous_usage: Option<&Usage>,
    ) {
        let reported_usage = match error {
            OpenAIError::JSONDeserialize(_, content) => {
                serde_json::from_str::<ChatCompletionUsage>(content)
                    .ok()
                    .and_then(|response| response.usage)
                    .map(|usage| Usage::from(&usage))
            }
            _ => None,
        };

        // Prefer the latest request total. Earlier stream usage is only a fallback, and is not
        // also reported on the success path: the stream terminates immediately after this error.
        self.track_completion(
            model,
            reported_usage.as_ref().or(previous_usage),
            Some(request),
            None::<&()>,
        );
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

#[cfg(feature = "langfuse")]
fn langfuse_json<T: Serialize + ?Sized>(value: &T) -> Option<String> {
    serde_json::to_string(value).ok()
}

fn tools_to_openai(spec: &ToolSpec) -> Result<ChatCompletionTools, LanguageModelError> {
    Ok(ChatCompletionTools::Function(
        async_openai::types::chat::ChatCompletionTool {
            function: FunctionObject {
                name: spec.name.clone(),
                description: Some(spec.description.clone()),
                parameters: Some(
                    spec.canonical_parameters_schema_json()
                        .map_err(LanguageModelError::permanent)?,
                ),
                strict: None,
            },
        },
    ))
}
