//! Stateless Mistral Responses. Reuses SDK item/usage types without changing the `OpenAI` adapter.
//! Upstream plaintext replay is available, but its integer timestamps, tool-ID normalization and
//! final-only reasoning emission do not cover this preview's contract. See compatibility tests.

use async_openai::{error::OpenAIError, types::responses as r};
use schemars::Schema;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;
use swiftide_core::chat_completion::{
    ChatCompletionRequest, ChatCompletionResponse, ReasoningItem, ReasoningStatus, ToolCall, Usage,
    errors::LanguageModelError,
};

use super::{Mistral, error::mistral_error};

mod request;
mod stream;

type Result<T> = std::result::Result<T, LanguageModelError>;

// Mistral's timestamps are numbers (not necessarily SDK u64 timestamps). Decode only the
// fields needed by Swiftide, reusing the unchanged SDK's complete output item and usage types.
#[derive(Deserialize)]
struct Response {
    output: Vec<r::OutputItem>,
    status: r::Status,
    usage: Option<r::ResponseUsage>,
    error: Option<r::ErrorObject>,
    incomplete_details: Option<Value>,
}

fn response_error(code: Option<&str>, message: &str) -> LanguageModelError {
    let description = format!("Mistral Responses error {code:?}: {message}");
    match code {
        Some("context_length_exceeded") => LanguageModelError::context_length_exceeded(description),
        Some(
            "invalid_request_error"
            | "authentication_error"
            | "invalid_prompt"
            | "content_filter"
            | "bio_policy"
            | "invalid_image"
            | "invalid_image_format"
            | "invalid_base64_image"
            | "invalid_image_url"
            | "image_too_large"
            | "image_too_small"
            | "image_parse_error"
            | "image_content_policy_violation"
            | "invalid_image_mode"
            | "image_file_too_large"
            | "unsupported_image_media_type"
            | "empty_image_file"
            | "image_file_not_found",
        ) => LanguageModelError::permanent(description),
        _ => LanguageModelError::transient(description),
    }
}

impl Response {
    fn into_completion(self) -> Result<ChatCompletionResponse> {
        if let Some(error) = self.error {
            return Err(response_error(Some(&error.code), &error.message));
        }
        match self.status {
            r::Status::Completed => {
                let mut response = output_to_completion(self.output.iter())?;
                response.usage = self.usage.as_ref().map(Usage::from);
                Ok(response)
            }
            r::Status::Incomplete => Err(LanguageModelError::permanent(format!(
                "Mistral Responses incomplete: {:?}",
                self.incomplete_details
            ))),
            status => Err(LanguageModelError::transient(format!(
                "Mistral Responses did not complete: {status:?}"
            ))),
        }
    }
}

impl Mistral {
    pub(super) async fn complete_responses(
        &self,
        request: &ChatCompletionRequest<'_>,
        schema: Option<Schema>,
    ) -> Result<ChatCompletionResponse> {
        let body = self.responses_request(request, false, schema)?;
        let model = self
            .default_options
            .prompt_model
            .as_deref()
            .expect("validated model");
        let result = self.client.responses().create_byot::<_, Value>(&body).await;
        let raw = match result {
            Ok(raw) => raw,
            Err(error) => {
                self.track_responses_error(model, &body, &error, None);
                return Err(mistral_error(error));
            }
        };
        let usage = response_usage(&raw);
        let result = decode::<Response>(&raw).and_then(Response::into_completion);
        self.track_completion(model, usage.as_ref(), Some(&body), result.as_ref().ok());
        result
    }

    fn track_responses_error(
        &self,
        model: &str,
        request: &Value,
        error: &OpenAIError,
        previous: Option<&Usage>,
    ) {
        let usage = match error {
            OpenAIError::JSONDeserialize(_, raw) => serde_json::from_str(raw)
                .ok()
                .and_then(|value| response_usage(&value)),
            _ => None,
        };
        self.track_completion(
            model,
            usage.as_ref().or(previous),
            Some(request),
            None::<&()>,
        );
    }
}

fn response_usage(raw: &Value) -> Option<Usage> {
    let raw = raw.get("response").unwrap_or(raw);
    serde_json::from_value::<r::ResponseUsage>(raw.get("usage")?.clone())
        .ok()
        .map(|usage| Usage::from(&usage))
}

fn decode<T: DeserializeOwned>(raw: &Value) -> Result<T> {
    serde_json::from_value(raw.clone()).map_err(|error| {
        // A valid Responses envelope/event with an unsupported schema cannot succeed unchanged.
        if raw["object"] == "response"
            || raw["type"]
                .as_str()
                .is_some_and(|t| t.starts_with("response."))
        {
            LanguageModelError::permanent(error)
        } else {
            mistral_error(OpenAIError::JSONDeserialize(error, raw.to_string()))
        }
    })
}

fn output_to_completion<'a>(
    items: impl IntoIterator<Item = &'a r::OutputItem>,
) -> Result<ChatCompletionResponse> {
    let mut response = ChatCompletionResponse::default();
    for item in items {
        match item {
            r::OutputItem::Message(message) => {
                for part in &message.content {
                    let text = match part {
                        r::OutputMessageContent::OutputText(text) => &text.text,
                        r::OutputMessageContent::Refusal(refusal) => &refusal.refusal,
                    };
                    response.message.get_or_insert_default().push_str(text);
                }
            }
            r::OutputItem::Reasoning(item) => {
                response
                    .reasoning
                    .get_or_insert_default()
                    .push(reasoning_from_sdk(item));
            }
            r::OutputItem::FunctionCall(call) => {
                if call.call_id.is_empty() || call.name.is_empty() || call.namespace.is_some() {
                    return Err(LanguageModelError::permanent(
                        "Mistral Responses requires named, non-namespaced function calls with call_id",
                    ));
                }
                response.tool_calls.get_or_insert_default().push(
                    ToolCall::builder()
                        .id(&call.call_id)
                        .name(&call.name)
                        .args(&call.arguments)
                        .build()
                        .map_err(LanguageModelError::permanent)?,
                );
            }
            _ => {
                return Err(LanguageModelError::permanent(
                    "Unsupported Mistral Responses output item",
                ));
            }
        }
    }
    Ok(response)
}

fn reasoning_from_sdk(item: &r::ReasoningItem) -> ReasoningItem {
    ReasoningItem {
        id: item.id.clone(),
        content: item.content.as_ref().map(|parts| {
            parts
                .iter()
                .map(|part| match part {
                    r::ReasoningItemContent::ReasoningText(text) => text.text.clone(),
                })
                .collect()
        }),
        summary: item
            .summary
            .iter()
            .map(|part| match part {
                r::SummaryPart::SummaryText(text) => text.text.clone(),
            })
            .collect(),
        encrypted_content: item.encrypted_content.clone(),
        status: item.status.as_ref().map(|status| match status {
            r::OutputStatus::Completed => ReasoningStatus::Completed,
            r::OutputStatus::Incomplete => ReasoningStatus::Incomplete,
            r::OutputStatus::InProgress => ReasoningStatus::InProgress,
        }),
    }
}

fn reasoning_to_sdk(item: &ReasoningItem) -> r::ReasoningItem {
    r::ReasoningItem {
        id: item.id.clone(),
        content: item.content.as_ref().map(|parts| {
            parts
                .iter()
                .map(|text| {
                    r::ReasoningItemContent::ReasoningText(r::ReasoningTextContent {
                        text: text.clone(),
                    })
                })
                .collect()
        }),
        summary: item
            .summary
            .iter()
            .map(|text| r::SummaryPart::SummaryText(r::SummaryTextContent { text: text.clone() }))
            .collect(),
        encrypted_content: item.encrypted_content.clone(),
        status: item.status.map(|status| match status {
            ReasoningStatus::Completed => r::OutputStatus::Completed,
            ReasoningStatus::Incomplete => r::OutputStatus::Incomplete,
            ReasoningStatus::InProgress => r::OutputStatus::InProgress,
        }),
    }
}
