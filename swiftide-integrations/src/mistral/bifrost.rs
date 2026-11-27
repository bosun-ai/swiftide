//! Only Bifrost's normalized Mistral plaintext reasoning, not arbitrary provider metadata.
//! Mistral's thinking text and optional closed flag fit the existing `ReasoningItem` unchanged.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use swiftide_core::chat_completion::{ReasoningItem, ReasoningStatus, errors::LanguageModelError};

#[derive(Debug, Default, Deserialize)]
pub(super) struct GatewayReasoning {
    reasoning: Option<String>,
    reasoning_content: Option<String>,
    reasoning_details: Option<Vec<ReasoningDetail>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
enum ReasoningDetail {
    #[serde(rename = "reasoning.text")]
    Text {
        text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        closed: Option<bool>,
    },
}

impl GatewayReasoning {
    pub(super) fn into_items(self) -> Option<Vec<ReasoningItem>> {
        if let Some(details) = self.reasoning_details.filter(|items| !items.is_empty()) {
            return Some(
                details
                    .into_iter()
                    .map(|ReasoningDetail::Text { text, closed }| ReasoningItem {
                        content: text.map(|text| vec![text]),
                        status: closed.map(|closed| {
                            if closed {
                                ReasoningStatus::Completed
                            } else {
                                ReasoningStatus::InProgress
                            }
                        }),
                        ..Default::default()
                    })
                    .collect(),
            );
        }
        // These aliases describe the same text; do not append them to the structured details.
        self.reasoning_content.or(self.reasoning).map(|text| {
            vec![ReasoningItem {
                content: Some(vec![text]),
                ..Default::default()
            }]
        })
    }
}

/// Translate already-encoded native assistant content into Bifrost's normalized history.
pub(super) fn encode_history(messages: &mut [Value]) -> Result<(), LanguageModelError> {
    for message in messages {
        if message["role"] != "assistant" {
            continue;
        }
        let Some(chunks) = message["content"].as_array() else {
            continue;
        };
        let mut details = Vec::new();
        let mut visible = None::<String>;
        let mut reasoning = String::new();
        for chunk in chunks {
            match serde_json::from_value::<super::wire::ContentChunk>(chunk.clone())
                .map_err(LanguageModelError::permanent)?
            {
                super::wire::ContentChunk::Text { text } => {
                    visible.get_or_insert_with(String::new).push_str(&text);
                }
                super::wire::ContentChunk::Thinking { thinking, closed } => {
                    let text: String = thinking
                        .into_iter()
                        .map(|super::wire::ThinkingText::Text { text }| text)
                        .collect();
                    reasoning.push_str(&text);
                    let mut detail = serde_json::to_value(ReasoningDetail::Text {
                        text: Some(text),
                        closed,
                    })
                    .map_err(LanguageModelError::permanent)?;
                    detail["index"] = json!(details.len());
                    details.push(detail);
                }
            }
        }
        message["content"] = json!(visible);
        if !details.is_empty() {
            message["reasoning_content"] = json!(reasoning);
            message["reasoning_details"] = json!(details);
        }
    }
    Ok(())
}
