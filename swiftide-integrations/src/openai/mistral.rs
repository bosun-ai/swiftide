//! Native Chat Completions types used with async-openai's BYOT transport.
//! Contract: <https://docs.mistral.ai/studio/conversations/reasoning> and
//! <https://docs.mistral.ai/studio/conversations/advanced/prompt-caching>.
//! Thinking is kept out of visible text and replayed as native assistant content, not as the
//! `reasoning_content` field used by some OpenAI-compatible providers.

use async_openai::types::chat::{
    ChatCompletionMessageToolCallChunk, ChatCompletionMessageToolCalls, CompletionUsage,
    CreateChatCompletionRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use swiftide_core::chat_completion::{
    ChatMessage, ReasoningItem, ReasoningStatus, errors::LanguageModelError,
};

use super::chat_completion::message_to_openai;
use super::chat_completion_adapter::{
    ChatCompletionAdapter, CompletionData, ContentData, DeltaData,
};

pub(super) struct MistralAdapter;

#[derive(Debug, Deserialize)]
pub(super) struct Completion {
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
pub(super) struct Chunk {
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

impl From<Content> for ContentData {
    fn from(content: Content) -> Self {
        let mut result = Self::default();
        let chunks = match content {
            Content::Text(text) => vec![ContentChunk::Text { text }],
            Content::Chunks(chunks) => chunks,
        };
        // One frame can contain both the end of thinking and the start of visible text.
        let mut text = None::<String>;
        for chunk in chunks {
            match chunk {
                ContentChunk::Text { text: part } => {
                    text.get_or_insert_with(String::new).push_str(&part);
                }
                ContentChunk::Thinking { thinking, closed } => {
                    result
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
        result.text = text;
        result
    }
}

impl From<Completion> for CompletionData {
    fn from(completion: Completion) -> Self {
        let mut result = Self {
            usage: completion.usage,
            ..Default::default()
        };
        if let Some(choice) = completion.choices.into_iter().next() {
            result.content = choice.message.content.map(Into::into).unwrap_or_default();
            result.tool_calls = choice.message.tool_calls;
        }
        result
    }
}

impl From<Chunk> for DeltaData {
    fn from(chunk: Chunk) -> Self {
        let mut result = Self {
            usage: chunk.usage,
            ..Default::default()
        };
        if let Some(choice) = chunk.choices.into_iter().next() {
            result.content = choice.delta.content.map(Into::into).unwrap_or_default();
            result.tool_calls = choice.delta.tool_calls;
        }
        result
    }
}

fn messages_to_mistral(messages: &[ChatMessage]) -> Result<Vec<Value>, LanguageModelError> {
    let mut result = Vec::new();
    let mut thinking = Vec::new();
    for message in messages {
        if let ChatMessage::Reasoning(item) = message {
            if item.provider_metadata.is_some() {
                return Err(LanguageModelError::permanent(
                    "Native Mistral cannot replay reasoning metadata from another wire format",
                ));
            }
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

impl ChatCompletionAdapter for MistralAdapter {
    type Completion = Completion;
    type Chunk = Chunk;

    fn request_body(
        request: CreateChatCompletionRequest,
        messages: &[ChatMessage],
    ) -> Result<Value, LanguageModelError> {
        let mut body = serde_json::to_value(request).map_err(LanguageModelError::permanent)?;
        let object = body.as_object_mut().expect("chat request is an object");
        object.remove("user");
        if let Some(max_tokens) = object.remove("max_completion_tokens") {
            object.insert("max_tokens".into(), max_tokens);
        }
        if let Some(seed) = object.remove("seed") {
            object.insert("random_seed".into(), seed);
        }
        object.insert("messages".into(), json!(messages_to_mistral(messages)?));
        Ok(body)
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
        let response = ContentData::from(content);
        assert!(response.text.is_none());
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

    #[test]
    fn gateway_metadata_is_not_reinterpreted_as_native_thinking() {
        let message = ChatMessage::new_reasoning(ReasoningItem {
            content: Some(vec!["gateway thinking".into()]),
            provider_metadata: Some(json!({"openai_chat_completions": {"reasoning_details": []}})),
            ..Default::default()
        });
        assert!(matches!(
            messages_to_mistral(&[message]),
            Err(LanguageModelError::PermanentError(_))
        ));
    }
}
