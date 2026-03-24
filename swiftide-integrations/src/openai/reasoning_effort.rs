use std::collections::HashSet;
use std::sync::Mutex;

use async_openai::error::OpenAIError;

/// Controls how `OpenAI` requests should handle `reasoning_effort`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReasoningEffortMode {
    /// Send `reasoning_effort` and retry once without it if the provider rejects it.
    #[default]
    Auto,
    /// Always send `reasoning_effort` and surface any provider rejection.
    Always,
    /// Never send `reasoning_effort`, even when it is configured.
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum ReasoningApi {
    ChatCompletions,
    Responses,
}

#[derive(Debug, Default)]
pub(super) struct ReasoningEffortCapabilities {
    unsupported: Mutex<HashSet<UnsupportedReasoningEffortKey>>,
}

impl ReasoningEffortCapabilities {
    pub(super) fn should_send(
        &self,
        mode: ReasoningEffortMode,
        is_configured: bool,
        api: ReasoningApi,
        model: &str,
    ) -> bool {
        if !is_configured {
            return false;
        }

        match mode {
            ReasoningEffortMode::Always => true,
            ReasoningEffortMode::Never => false,
            ReasoningEffortMode::Auto => {
                !self.is_known_unsupported(&UnsupportedReasoningEffortKey::new(api, model))
            }
        }
    }

    pub(super) fn should_retry_without_reasoning_effort(
        &self,
        mode: ReasoningEffortMode,
        api: ReasoningApi,
        model: &str,
        error: &OpenAIError,
    ) -> bool {
        if mode != ReasoningEffortMode::Auto || !rejects_reasoning_effort(error) {
            return false;
        }

        self.mark_unsupported(UnsupportedReasoningEffortKey::new(api, model));
        true
    }

    fn is_known_unsupported(&self, key: &UnsupportedReasoningEffortKey) -> bool {
        self.unsupported
            .lock()
            .expect("reasoning effort capability cache should not be poisoned")
            .contains(key)
    }

    fn mark_unsupported(&self, key: UnsupportedReasoningEffortKey) {
        self.unsupported
            .lock()
            .expect("reasoning effort capability cache should not be poisoned")
            .insert(key);
    }
}

fn rejects_reasoning_effort(error: &OpenAIError) -> bool {
    let OpenAIError::ApiError(api_error) = error else {
        return false;
    };

    let param_mentions_reasoning = api_error
        .param
        .as_deref()
        .is_some_and(|param| matches!(param, "reasoning_effort" | "reasoning.effort"));

    let message = api_error.message.to_ascii_lowercase();
    let message_mentions_reasoning =
        message.contains("reasoning_effort") || message.contains("reasoning effort");
    let message_mentions_unsupported = message.contains("unrecognized request argument")
        || message.contains("unsupported")
        || message.contains("does not support")
        || message.contains("not supported");

    param_mentions_reasoning || (message_mentions_reasoning && message_mentions_unsupported)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct UnsupportedReasoningEffortKey {
    api: ReasoningApi,
    model: String,
}

impl UnsupportedReasoningEffortKey {
    fn new(api: ReasoningApi, model: &str) -> Self {
        Self {
            api,
            model: model.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use async_openai::error::{ApiError, OpenAIError};

    use super::{ReasoningApi, ReasoningEffortCapabilities, ReasoningEffortMode};

    #[test]
    fn auto_mode_stops_sending_after_reasoning_effort_rejection() {
        let capabilities = ReasoningEffortCapabilities::default();

        assert!(capabilities.should_send(
            ReasoningEffortMode::Auto,
            true,
            ReasoningApi::ChatCompletions,
            "gpt-4.1"
        ));

        let error = OpenAIError::ApiError(ApiError {
            message: "Unrecognized request argument supplied: reasoning_effort".to_string(),
            r#type: Some("invalid_request_error".to_string()),
            param: Some("reasoning_effort".to_string()),
            code: None,
        });

        assert!(capabilities.should_retry_without_reasoning_effort(
            ReasoningEffortMode::Auto,
            ReasoningApi::ChatCompletions,
            "gpt-4.1",
            &error,
        ));
        assert!(!capabilities.should_send(
            ReasoningEffortMode::Auto,
            true,
            ReasoningApi::ChatCompletions,
            "gpt-4.1"
        ));
    }

    #[test]
    fn always_mode_does_not_downgrade_on_reasoning_effort_rejection() {
        let capabilities = ReasoningEffortCapabilities::default();

        let error = OpenAIError::ApiError(ApiError {
            message: "Unrecognized request argument supplied: reasoning_effort".to_string(),
            r#type: Some("invalid_request_error".to_string()),
            param: Some("reasoning_effort".to_string()),
            code: None,
        });

        assert!(!capabilities.should_retry_without_reasoning_effort(
            ReasoningEffortMode::Always,
            ReasoningApi::ChatCompletions,
            "gpt-4.1",
            &error,
        ));
    }
}
