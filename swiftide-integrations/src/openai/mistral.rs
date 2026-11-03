//! Native Chat Completions types used with async-openai's BYOT transport.
//! Contract: <https://docs.mistral.ai/studio/conversations/reasoning> and
//! <https://docs.mistral.ai/studio/conversations/advanced/prompt-caching>.
//! Thinking is kept out of visible text and replayed as native assistant content, not as the
//! `reasoning_content` field used by some OpenAI-compatible providers.

use std::sync::Arc;

use async_openai::types::chat::{
    ChatCompletionMessageToolCallChunk, ChatCompletionMessageToolCalls, CompletionUsage,
};
use futures_util::{StreamExt as _, stream};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use swiftide_core::ChatCompletionStream;
use swiftide_core::chat_completion::{
    ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ReasoningItem, ReasoningStatus,
    ToolCall, Usage, errors::LanguageModelError,
};
use tracing_futures::Instrument as _;

use super::chat_completion::{message_to_openai, tools_to_openai};
use super::{GenericOpenAI, openai_error_to_language_model_error, request_body_with_extra_body};

#[derive(Debug, Deserialize)]
struct Completion {
    choices: Vec<Choice>,
    usage: Option<CompletionUsage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Debug, Deserialize)]
struct Message {
    content: Option<Content>,
    tool_calls: Option<Vec<ChatCompletionMessageToolCalls>>,
}

#[derive(Debug, Deserialize)]
struct Chunk {
    choices: Vec<StreamChoice>,
    usage: Option<CompletionUsage>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: Delta,
}

#[derive(Debug, Deserialize)]
struct Delta {
    content: Option<Content>,
    tool_calls: Option<Vec<ChatCompletionMessageToolCallChunk>>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Content {
    Text(String),
    Chunks(Vec<ContentChunk>),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentChunk {
    Text {
        text: String,
    },
    Thinking {
        thinking: Vec<ThinkingText>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        closed: Option<bool>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ThinkingText {
    Text { text: String },
}

impl Content {
    fn apply(self, response: &mut ChatCompletionResponse) {
        let chunks = match self {
            Self::Text(text) => vec![ContentChunk::Text { text }],
            Self::Chunks(chunks) => chunks,
        };
        // One frame can contain both the end of thinking and the start of visible text.
        let mut text = None::<String>;
        for chunk in chunks {
            match chunk {
                ContentChunk::Text { text: part } => {
                    text.get_or_insert_with(String::new).push_str(&part);
                }
                ContentChunk::Thinking { thinking, closed } => {
                    response
                        .reasoning
                        .get_or_insert_with(Vec::new)
                        .push(ReasoningItem {
                            content: Some(
                                thinking
                                    .into_iter()
                                    .map(|ThinkingText::Text { text }| text)
                                    .collect(),
                            ),
                            status: closed.map(|closed| {
                                if closed {
                                    ReasoningStatus::Completed
                                } else {
                                    ReasoningStatus::InProgress
                                }
                            }),
                            ..Default::default()
                        });
                }
            }
        }
        response.append_message_delta(text.as_deref());
    }
}

impl Chunk {
    fn apply(self, state: &mut ChatCompletionResponse) -> Option<Vec<ReasoningItem>> {
        let mut fragment = ChatCompletionResponse::default();
        if let Some(choice) = self.choices.into_iter().next() {
            if let Some(content) = choice.delta.content {
                content.apply(&mut fragment);
            }
            state.append_message_delta(fragment.message.as_deref());
            if let Some(items) = &fragment.reasoning {
                // Thinking text spans frames, sometimes with closed=true on every fragment.
                // Concatenate text rather than treating SSE boundaries as reasoning boundaries.
                let accumulated = state.reasoning.get_or_insert_with(Vec::new);
                for item in items {
                    if let Some(previous) = accumulated.last_mut() {
                        let text = previous.content.get_or_insert_with(Vec::new);
                        if text.is_empty() {
                            text.push(String::new());
                        }
                        for part in item.content.iter().flatten() {
                            text.last_mut().expect("nonempty").push_str(part);
                        }
                        previous.status = item.status;
                    } else {
                        accumulated.push(item.clone());
                    }
                }
            }
            for call in choice.delta.tool_calls.into_iter().flatten() {
                state.append_tool_call_delta(
                    call.index as usize,
                    call.id.as_deref(),
                    call.function.as_ref().and_then(|f| f.name.as_deref()),
                    call.function.as_ref().and_then(|f| f.arguments.as_deref()),
                );
            }
        }
        if let Some(usage) = &self.usage {
            state.usage = Some(Usage::from(usage));
        }
        fragment.reasoning
    }
}

fn messages_to_mistral(messages: &[ChatMessage]) -> Result<Vec<Value>, LanguageModelError> {
    let mut result = Vec::new();
    let mut thinking = Vec::new();
    for message in messages {
        if let ChatMessage::Reasoning(item) = message {
            if item.encrypted_content.is_some() {
                return Err(LanguageModelError::permanent(
                    "Mistral cannot replay encrypted reasoning from another provider",
                ));
            }
            if let Some(content) = &item.content {
                thinking.push(ContentChunk::Thinking {
                    thinking: content
                        .iter()
                        .map(|text| ThinkingText::Text { text: text.clone() })
                        .collect(),
                    closed: item
                        .status
                        .map(|status| status == ReasoningStatus::Completed),
                });
            }
            continue;
        }

        if !thinking.is_empty() && !matches!(message, ChatMessage::Assistant(..)) {
            result.push(json!({"role": "assistant", "content": std::mem::take(&mut thinking)}));
        }
        let converted = message_to_openai(message)?;
        let mut converted = match converted {
            Some(message) => {
                serde_json::to_value(message).map_err(LanguageModelError::permanent)?
            }
            None if !thinking.is_empty() => json!({"role": "assistant"}),
            None => continue,
        };
        if !thinking.is_empty() {
            if let ChatMessage::Assistant(Some(text), _) = message {
                thinking.push(ContentChunk::Text { text: text.clone() });
            }
            converted["content"] = serde_json::to_value(std::mem::take(&mut thinking))
                .map_err(LanguageModelError::permanent)?;
        }
        result.push(converted);
    }
    if !thinking.is_empty() {
        result.push(json!({"role": "assistant", "content": thinking}));
    }
    Ok(result)
}

impl<C> GenericOpenAI<C>
where
    C: async_openai::config::Config + Default + Sync + Send + std::fmt::Debug + Clone + 'static,
{
    fn mistral_request(
        &self,
        request: &ChatCompletionRequest<'_>,
        streaming: bool,
    ) -> Result<Value, LanguageModelError> {
        if self.use_responses_api {
            return Err(LanguageModelError::permanent(
                "Mistral Chat Completions format cannot be combined with use_responses_api",
            ));
        }
        let model = self
            .default_options
            .prompt_model
            .as_ref()
            .ok_or_else(|| LanguageModelError::permanent("Model not set"))?;
        let mut builder = self.chat_completion_request_defaults();
        builder
            .model(model)
            .messages(Vec::<async_openai::types::chat::ChatCompletionRequestMessage>::new());
        if !request.tools_spec().is_empty() {
            builder.tools(
                request
                    .tools_spec()
                    .iter()
                    .map(tools_to_openai)
                    .collect::<anyhow::Result<Vec<_>>>()?,
            );
        }
        let request_defaults = builder
            .build()
            .map_err(openai_error_to_language_model_error)?;
        let mut body =
            serde_json::to_value(request_defaults).map_err(LanguageModelError::permanent)?;
        let object = body.as_object_mut().expect("chat request is an object");
        object.remove("user");
        if let Some(max_tokens) = object.remove("max_completion_tokens") {
            object.insert("max_tokens".into(), max_tokens);
        }
        if let Some(seed) = object.remove("seed") {
            object.insert("random_seed".into(), seed);
        }
        object.insert(
            "messages".into(),
            json!(messages_to_mistral(request.messages())?),
        );
        if streaming {
            object.insert("stream".into(), json!(true));
            object.insert("stream_options".into(), json!({"include_usage": true}));
        }
        request_body_with_extra_body(&body, &self.default_options.extra_body)
    }

    pub(super) async fn complete_mistral(
        &self,
        request: &ChatCompletionRequest<'_>,
    ) -> Result<ChatCompletionResponse, LanguageModelError> {
        let body = self.mistral_request(request, false)?;
        let model = self
            .default_options
            .prompt_model
            .as_deref()
            .expect("validated model");
        let completion: Completion =
            self.client
                .chat()
                .create_byot(&body)
                .await
                .map_err(|error| {
                    self.track_failed_chat_completion(model, &error, &body, None);
                    openai_error_to_language_model_error(error)
                })?;
        let mut response = ChatCompletionResponse::default();
        if let Some(choice) = completion.choices.into_iter().next() {
            if let Some(content) = choice.message.content {
                content.apply(&mut response);
            }
            response.tool_calls = choice.message.tool_calls.map(|calls| {
                calls
                    .into_iter()
                    .filter_map(|call| match call {
                        ChatCompletionMessageToolCalls::Function(call) => Some(
                            ToolCall::builder()
                                .id(call.id)
                                .name(call.function.name)
                                .args(call.function.arguments)
                                .build()
                                .expect("infallible"),
                        ),
                        ChatCompletionMessageToolCalls::Custom(_) => None,
                    })
                    .collect()
            });
        }
        response.delta = None;
        response.usage = completion.usage.as_ref().map(Usage::from);
        self.track_completion(model, response.usage.as_ref(), Some(&body), Some(&response));
        Ok(response)
    }

    pub(super) async fn complete_stream_mistral(
        &self,
        request: &ChatCompletionRequest<'_>,
    ) -> Result<ChatCompletionStream, LanguageModelError> {
        let body = self.mistral_request(request, true)?;
        let model = self
            .default_options
            .prompt_model
            .clone()
            .expect("validated model");
        let source = self
            .client
            .chat()
            .create_stream_byot::<_, Chunk>(&body)
            .await
            .map_err(|error| {
                self.track_failed_chat_completion(&model, &error, &body, None);
                openai_error_to_language_model_error(error)
            })?;
        let client = self.clone();
        let body = Arc::new(body);
        let span = if cfg!(feature = "langfuse") {
            tracing::info_span!("mistral_stream", langfuse.type = "GENERATION")
        } else {
            tracing::info_span!("mistral_stream")
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
                    // Tool buffers live in delta; only the visible text delta is per-frame.
                    if let Some(delta) = &mut state.delta {
                        delta.message_chunk = None;
                    }
                    match source.next().await {
                        Some(Ok(chunk)) => {
                            let reasoning = chunk.apply(&mut state);
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
                            Some((
                                Err(openai_error_to_language_model_error(error)),
                                (source, state, true),
                            ))
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case(None; "missing closed")]
    #[test_case(Some(false); "open thinking")]
    #[test_case(Some(true); "closed thinking")]
    fn thinking_only_history_retains_nested_text_and_status(closed: Option<bool>) {
        let chunk = ContentChunk::Thinking {
            thinking: vec![
                ThinkingText::Text {
                    text: "First pensée.".into(),
                },
                ThinkingText::Text {
                    text: " Second.".into(),
                },
            ],
            closed,
        };
        let expected = json!([chunk]);
        let content: Content = serde_json::from_value(expected.clone()).unwrap();
        let mut response = ChatCompletionResponse::default();
        content.apply(&mut response);
        assert!(response.message.is_none());
        let mut messages: Vec<_> = response
            .reasoning
            .unwrap()
            .into_iter()
            .map(ChatMessage::new_reasoning)
            .collect();
        // Reasoning-only completions have no following Assistant in agent history.
        assert_eq!(
            messages_to_mistral(&messages).unwrap(),
            vec![json!({"role": "assistant", "content": expected})]
        );
        messages.push(ChatMessage::new_user("Continue"));
        let converted = messages_to_mistral(&messages).unwrap();
        assert_eq!(
            converted[0],
            json!({"role": "assistant", "content": expected})
        );
        assert_eq!(converted[1], json!({"role": "user", "content": "Continue"}));
    }
}
