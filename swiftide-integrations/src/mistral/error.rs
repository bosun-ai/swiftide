//! Error classification is local to Mistral; it does not change the standard `OpenAI` client.
use async_openai::error::OpenAIError;
use serde::Deserialize;
use swiftide_core::chat_completion::errors::LanguageModelError;

#[derive(Deserialize)]
#[serde(tag = "object", rename_all = "snake_case")]
enum NativeError {
    Error {
        #[serde(rename = "type")]
        kind: String,
    },
}

#[derive(Deserialize)]
struct CompletionEnvelope {
    #[serde(rename = "choices")]
    _choices: Vec<serde::de::IgnoredAny>,
}

#[derive(Deserialize)]
struct ErrorEnvelope {
    error: Option<serde::de::IgnoredAny>,
    #[serde(default)]
    choices: Vec<ChoiceError>,
}

#[derive(Deserialize)]
struct ChoiceError {
    error: Option<serde::de::IgnoredAny>,
}

// Mistral documents four top-level error categories:
// https://docs.mistral.ai/resources/error-glossary#error-response-format
// Invalid requests and authentication errors cannot succeed unchanged. Rate limits and server
// errors are retryable. async-openai discards HTTP status on unrecognized error bodies, so
// unknown bodies stay transient; only a recognizable complete completion schema is permanent.
pub(super) fn mistral_error(error: OpenAIError) -> LanguageModelError {
    match &error {
        OpenAIError::JSONDeserialize(_, content) => {
            match serde_json::from_str::<NativeError>(content) {
                Ok(NativeError::Error { kind })
                    if matches!(
                        kind.as_str(),
                        "invalid_request_error" | "authentication_error"
                    ) =>
                {
                    LanguageModelError::permanent(error)
                }
                Ok(_) => LanguageModelError::transient(error),
                Err(_)
                    if serde_json::from_str::<ErrorEnvelope>(content).is_ok_and(|response| {
                        response.error.is_some()
                            || response.choices.iter().any(|choice| choice.error.is_some())
                    }) =>
                {
                    crate::openai::openai_error_to_language_model_error(error)
                }
                Err(_) if serde_json::from_str::<CompletionEnvelope>(content).is_ok() => {
                    LanguageModelError::permanent(error)
                }
                Err(_) => crate::openai::openai_error_to_language_model_error(error),
            }
        }
        // Reuse the existing public mapper unchanged for transport and compatible gateway errors.
        _ => crate::openai::openai_error_to_language_model_error(error),
    }
}
