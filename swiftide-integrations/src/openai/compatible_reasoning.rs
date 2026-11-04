//! Reasoning extensions used by OpenAI-compatible gateways, not `OpenAI`'s Chat Completions API.
//! The standard schema remains validated by async-openai. Bifrost exposes `reasoning` and
//! `reasoning_content` aliases plus structured `reasoning_details` (including Mistral's `closed`).
//! Keep the original fields for replay; do not infer a provider from its model name or base URL.
//! Contract: Bifrost core/schemas/chatcompletions.go (`ChatReasoningDetails` /
//! `ChatAssistantMessage`) and <https://openrouter.ai/docs/guides/best-practices/reasoning-tokens>.

use async_openai::types::chat::{CreateChatCompletionResponse, CreateChatCompletionStreamResponse};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use swiftide_core::chat_completion::{
    ChatMessage, ReasoningItem, ReasoningStatus, errors::LanguageModelError,
};

use super::chat_completion::message_to_openai;
use super::chat_completion_adapter::{CompletionData, DeltaData};

const METADATA_KEY: &str = "openai_chat_completions";

#[derive(Deserialize)]
#[serde(try_from = "Value")]
pub(super) struct Completion(CompletionData);

impl TryFrom<Value> for Completion {
    type Error = serde_json::Error;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let mut data = CompletionData::from(CreateChatCompletionResponse::deserialize(&value)?);
        data.content.reasoning = value
            .pointer("/choices/0/message")
            .map(ReasoningFields::deserialize)
            .transpose()?
            .and_then(ReasoningFields::into_item)
            .map(|item| vec![item]);
        Ok(Self(data))
    }
}

impl From<Completion> for CompletionData {
    fn from(completion: Completion) -> Self {
        completion.0
    }
}

#[derive(Deserialize)]
#[serde(try_from = "Value")]
pub(super) struct Chunk(DeltaData);

impl TryFrom<Value> for Chunk {
    type Error = serde_json::Error;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let mut data = DeltaData::from(CreateChatCompletionStreamResponse::deserialize(&value)?);
        data.content.reasoning = value
            .pointer("/choices/0/delta")
            .map(ReasoningFields::deserialize)
            .transpose()?
            .and_then(ReasoningFields::into_item)
            .map(|item| vec![item]);
        Ok(Self(data))
    }
}

impl From<Chunk> for DeltaData {
    fn from(chunk: Chunk) -> Self {
        chunk.0
    }
}

#[derive(Default, Serialize, Deserialize)]
struct ReasoningFields {
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_content: Option<String>,
    // Retain signatures, IDs, format tags, closed=false, and unknown provider metadata verbatim.
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_details: Option<Vec<Map<String, Value>>>,
}

impl ReasoningFields {
    fn into_item(self) -> Option<ReasoningItem> {
        if self.reasoning.is_none()
            && self.reasoning_content.is_none()
            && self.reasoning_details.is_none()
        {
            return None;
        }
        let details = self.reasoning_details.iter().flatten();
        let mut text: Vec<_> = details
            .clone()
            .filter_map(|detail| {
                detail
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .collect();
        // Details and aliases describe the same reasoning, not three separate thoughts.
        if text.is_empty() {
            text.extend(
                self.reasoning_content
                    .as_ref()
                    .or(self.reasoning.as_ref())
                    .cloned(),
            );
        }
        let summary = details
            .clone()
            .filter_map(|detail| {
                detail
                    .get("summary")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .collect();
        let status = details
            .filter_map(|detail| detail.get("closed").and_then(Value::as_bool))
            .next_back()
            .map(|closed| {
                if closed {
                    ReasoningStatus::Completed
                } else {
                    ReasoningStatus::InProgress
                }
            });
        Some(ReasoningItem {
            content: (!text.is_empty()).then_some(text),
            summary,
            status,
            provider_metadata: Some(json!({METADATA_KEY: self})),
            ..Default::default()
        })
    }

    fn from_item(item: &ReasoningItem) -> Result<Option<Self>, serde_json::Error> {
        item.provider_metadata
            .as_ref()
            .and_then(|metadata| metadata.get(METADATA_KEY))
            .map(Self::deserialize)
            .transpose()
    }

    fn into_message(self, message: Value) -> Value {
        #[derive(Serialize)]
        struct MessageWithReasoning {
            #[serde(flatten)]
            message: Value,
            #[serde(flatten)]
            reasoning: ReasoningFields,
        }
        serde_json::to_value(MessageWithReasoning {
            message,
            reasoning: self,
        })
        .expect("serializable assistant message and reasoning fields")
    }

    fn append(&mut self, next: Self) {
        for (current, fragment) in [
            (&mut self.reasoning, next.reasoning),
            (&mut self.reasoning_content, next.reasoning_content),
        ] {
            if let Some(fragment) = fragment {
                current.get_or_insert_with(String::new).push_str(&fragment);
            }
        }
        if let Some(details) = next.reasoning_details {
            let accumulated = self.reasoning_details.get_or_insert_with(Vec::new);
            for detail in details {
                // Bifrost/OpenRouter index individual blocks. Distinct types at one index must
                // not overwrite one another; absent IDs on later fragments retain the first ID.
                let previous = accumulated.iter_mut().find(|previous| {
                    previous.get("type") == detail.get("type")
                        && match (
                            previous.get("id").and_then(Value::as_str),
                            detail.get("id").and_then(Value::as_str),
                        ) {
                            (Some(left), Some(right)) => left == right,
                            _ => true,
                        }
                        && match (previous.get("index"), detail.get("index")) {
                            (Some(left), Some(right)) => left == right,
                            (None, None) => previous.get("id") == detail.get("id"),
                            _ => false,
                        }
                });
                if let Some(previous) = previous {
                    for (key, value) in detail {
                        // Null optional fields in deltas mean no new value, not deletion of
                        // previously received text, signatures, identity, or completion state.
                        if value.is_null()
                            && previous.contains_key(&key)
                            && matches!(
                                key.as_str(),
                                "text"
                                    | "summary"
                                    | "data"
                                    | "signature"
                                    | "id"
                                    | "index"
                                    | "type"
                                    | "format"
                                    | "closed"
                            )
                        {
                            continue;
                        }
                        if matches!(key.as_str(), "text" | "summary" | "data" | "signature")
                            && let Some(Value::String(text)) = previous.get_mut(&key)
                            && let Some(fragment) = value.as_str()
                        {
                            text.push_str(fragment);
                        } else {
                            previous.insert(key, value);
                        }
                    }
                } else {
                    accumulated.push(detail);
                }
            }
        }
    }
}

pub(super) fn accumulate_reasoning(accumulated: &mut Vec<ReasoningItem>, items: &[ReasoningItem]) {
    for item in items {
        if let Some(previous) = accumulated.last_mut() {
            let mut fields = ReasoningFields::from_item(previous)
                .expect("decoded reasoning metadata")
                .expect("compatible reasoning metadata");
            fields.append(
                ReasoningFields::from_item(item)
                    .expect("decoded reasoning metadata")
                    .expect("compatible reasoning metadata"),
            );
            *previous = fields.into_item().expect("retained reasoning fields");
        } else {
            accumulated.push(item.clone());
        }
    }
}

pub(super) fn messages_to_openai(
    messages: &[ChatMessage],
) -> Result<Vec<Value>, LanguageModelError> {
    let mut result = Vec::new();
    let mut pending = None::<ReasoningFields>;
    for message in messages {
        if let ChatMessage::Reasoning(item) = message {
            if let Some(fields) =
                ReasoningFields::from_item(item).map_err(LanguageModelError::permanent)?
            {
                // Each item is one completed response, not an SSE fragment. Adjacent
                // reasoning-only turns must not merge blocks that reuse the same index.
                if let Some(previous) = pending.replace(fields) {
                    result
                        .push(previous.into_message(json!({"role": "assistant", "content": null})));
                }
            }
            continue;
        }
        if !matches!(message, ChatMessage::Assistant(..))
            && let Some(fields) = pending.take()
        {
            result.push(fields.into_message(json!({"role": "assistant", "content": null})));
        }
        let converted = message_to_openai(message)?;
        let mut converted = match converted {
            Some(message) => {
                serde_json::to_value(message).map_err(LanguageModelError::permanent)?
            }
            None if pending.is_some() => json!({"role": "assistant", "content": null}),
            None => continue,
        };
        if let Some(fields) = pending.take() {
            converted = fields.into_message(converted);
        }
        result.push(converted);
    }
    if let Some(fields) = pending {
        result.push(fields.into_message(json!({"role": "assistant", "content": null})));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case(&json!({"reasoning": "pensée"}), Some("pensée"); "reasoning alias")]
    #[test_case(&json!({"reasoning_content": "pensée"}), Some("pensée"); "reasoning content alias")]
    #[test_case(&json!({"reasoning": "pensée", "reasoning_content": "pensée"}), Some("pensée"); "both aliases")]
    #[test_case(&json!({"reasoning_content": "", "reasoning_details": null}), Some(""); "empty reasoning")]
    #[test_case(&json!({"reasoning_details": []}), None; "empty details")]
    #[test_case(&json!({"reasoning_details": [
        {"index": 0, "type": "reasoning.summary", "summary": "Summary", "id": null},
        {"index": 1, "type": "reasoning.encrypted", "data": "opaque", "format": "v1"},
        {"index": 2, "type": "future.type", "unknown": {"keep": [1, 2]}, "closed": null}
    ]}), None; "opaque details")]
    fn reasoning_only_history_preserves_fields(fields: &Value, text: Option<&str>) {
        let item = ReasoningFields::deserialize(fields)
            .unwrap()
            .into_item()
            .unwrap();
        assert_eq!(item.content, text.map(|text| vec![text.to_owned()]));
        // Gateway encrypted payloads must not masquerade as native Responses API ciphertext.
        assert!(item.encrypted_content.is_none());
        let mut messages = vec![ChatMessage::new_reasoning(item)];
        for suffix in [
            None,
            Some(ChatMessage::new_assistant(None::<String>, None)),
            Some(ChatMessage::new_user("continue")),
        ] {
            if let Some(suffix) = suffix {
                messages.push(suffix);
            }
            let replay = messages_to_openai(&messages).unwrap();
            assert_eq!(replay[0]["role"], "assistant");
            assert_eq!(replay[0]["content"], Value::Null);
            for (key, value) in fields.as_object().unwrap() {
                assert_eq!(&replay[0][key], value);
            }
        }
    }

    #[test]
    fn adjacent_reasoning_only_turns_do_not_merge_detail_indices() {
        let make = |text| {
            ReasoningFields::deserialize(json!({"reasoning_details": [
                {"index": 0, "type": "reasoning.text", "text": text}
            ]}))
            .unwrap()
            .into_item()
            .unwrap()
        };
        let messages = vec![
            ChatMessage::new_reasoning(make("first")),
            ChatMessage::new_reasoning(make("second")),
            ChatMessage::new_user("continue"),
        ];
        let replay = messages_to_openai(&messages).unwrap();
        assert_eq!(replay.len(), 3);
        assert_eq!(replay[0]["reasoning_details"][0]["text"], "first");
        assert_eq!(replay[1]["reasoning_details"][0]["text"], "second");
        assert_eq!(replay[2]["role"], "user");
    }

    #[test]
    fn ordinary_openai_history_and_foreign_reasoning_remain_unchanged() {
        let legacy: ReasoningItem =
            serde_json::from_value(json!({"summary": [], "content": ["native thought"]})).unwrap();
        assert!(legacy.provider_metadata.is_none());
        assert!(
            serde_json::to_value(&legacy)
                .unwrap()
                .get("provider_metadata")
                .is_none()
        );
        let messages = vec![
            ChatMessage::new_user("hi"),
            ChatMessage::new_reasoning(legacy),
            ChatMessage::new_assistant(Some("hello"), None),
            ChatMessage::new_assistant(None::<String>, None),
        ];
        let expected: Vec<_> = messages
            .iter()
            .filter_map(|message| message_to_openai(message).unwrap())
            .map(|message| serde_json::to_value(message).unwrap())
            .collect();
        assert_eq!(messages_to_openai(&messages).unwrap(), expected);
    }

    #[test]
    fn indexed_stream_details_keep_distinct_blocks_and_opaque_metadata() {
        let mut fields = ReasoningFields::deserialize(json!({"reasoning_details": [
            {"index": 0, "type": "reasoning.text", "text": "one", "id": "a", "closed": false},
            {"index": 1, "type": "reasoning.text", "text": "two", "id": "b"},
            {"index": 0, "type": "reasoning.summary", "summary": "sum", "format": "v1"}
        ]}))
        .unwrap();
        fields.append(
            ReasoningFields::deserialize(json!({"reasoning_details": [
                {"index": 1, "type": "reasoning.text", "text": "!"},
            {"index": 0, "type": "reasoning.text", "signature": "sig", "id": null, "text": null, "closed": null},
                {"index": 0, "type": "reasoning.summary", "summary": "mary"},
                {"index": 0, "type": "reasoning.text", "id": "new", "text": "different block"}
            ]}))
            .unwrap(),
        );
        let item = fields.into_item().unwrap();
        assert_eq!(item.content.unwrap(), ["one", "two!", "different block"]);
        assert_eq!(item.summary, ["summary"]);
        assert_eq!(item.status, Some(ReasoningStatus::InProgress));
        let details = &item.provider_metadata.unwrap()[METADATA_KEY]["reasoning_details"];
        assert_eq!(details.as_array().unwrap().len(), 4);
        assert_eq!(details[0]["id"], "a");
        assert_eq!(details[0]["signature"], "sig");
        assert_eq!(details[2]["format"], "v1");
    }
}
