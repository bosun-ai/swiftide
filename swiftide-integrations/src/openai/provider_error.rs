use std::fmt;

use async_openai::error::OpenAIError;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::Number;
use swiftide_core::chat_completion::errors::LanguageModelError;

// OpenAI transports billing, spend, and quota failures as HTTP 429, but explicitly documents
// them as non-retryable:
// https://developers.openai.com/api/docs/guides/error-codes#api-errors
const OPENAI_NON_RETRYABLE_QUOTA_ERRORS: &[&str] = &[
    "insufficient_quota",
    "credit_balance_exhausted",
    "organization_spend_limit_exceeded",
    "project_spend_limit_exceeded",
    "organization_usage_limit_exceeded",
];

// Canonical retryable OpenRouter error_type values for Chat Completions:
// https://openrouter.ai/docs/api_reference/errors-and-debugging#typed-error-codes
const OPENROUTER_TRANSIENT_ERROR_TYPES: &[&str] = &[
    "rate_limit_exceeded",
    "provider_overloaded",
    "provider_unavailable",
    "server",
    "timeout",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorClassification {
    ContextLengthExceeded,
    Permanent,
    Transient,
}

#[derive(Debug, Deserialize)]
struct ProviderErrorResponse {
    error: Option<ProviderErrorBody>,
    #[serde(default)]
    choices: Vec<ProviderErrorChoice>,
}

#[derive(Debug, Deserialize)]
struct ProviderErrorChoice {
    error: Option<ProviderErrorBody>,
}

#[derive(Debug, Deserialize)]
struct ProviderErrorBody {
    message: String,
    code: Option<StringOrNumber>,
    #[serde(rename = "type")]
    api_type: Option<String>,
    error_type: Option<String>,
    metadata: Option<ProviderErrorMetadata>,
}

#[derive(Debug, Default, Deserialize)]
struct ProviderErrorMetadata {
    error_type: Option<String>,
    provider_code: Option<StringOrNumber>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum StringOrNumber {
    String(String),
    Number(Number),
}

impl fmt::Display for StringOrNumber {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(value) => formatter.write_str(value),
            Self::Number(value) => fmt::Display::fmt(value, formatter),
        }
    }
}

#[derive(Debug)]
struct ProviderError {
    message: String,
    code: Option<String>,
    error_type: Option<String>,
    provider_code: Option<String>,
}

impl ProviderError {
    fn from_response(content: &str) -> Option<Self> {
        let response: ProviderErrorResponse = serde_json::from_str(content).ok()?;
        let error = response
            .error
            .or_else(|| response.choices.into_iter().find_map(|choice| choice.error))?;
        let metadata = error.metadata.unwrap_or_default();
        let error_type = metadata.error_type.or(error.error_type).or(error.api_type);

        Some(Self {
            message: error.message,
            code: error.code.map(|code| code.to_string()),
            error_type,
            provider_code: metadata.provider_code.map(|code| code.to_string()),
        })
    }

    fn classification(&self) -> ErrorClassification {
        classify_error(
            status_from_code(self.code.as_deref()),
            [
                self.code.as_deref(),
                self.error_type.as_deref(),
                self.provider_code.as_deref(),
            ],
        )
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "provider error: {}", self.message)?;

        let mut separator = " (";
        for (name, value) in [
            ("code", self.code.as_deref()),
            ("type", self.error_type.as_deref()),
            ("provider code", self.provider_code.as_deref()),
        ] {
            if let Some(value) = value {
                write!(formatter, "{separator}{name}: {value}")?;
                separator = ", ";
            }
        }

        if separator == ", " {
            write!(formatter, ")")?;
        }

        Ok(())
    }
}

impl std::error::Error for ProviderError {}

pub(super) fn openai_error_to_language_model_error(e: OpenAIError) -> LanguageModelError {
    match e {
        OpenAIError::ApiError(api_error) => {
            let classification = classify_error(
                Some(api_error.status_code),
                [
                    api_error.api_error.code.as_deref(),
                    api_error.api_error.r#type.as_deref(),
                    None,
                ],
            );

            map_classification(classification, OpenAIError::ApiError(api_error))
        }
        OpenAIError::Reqwest(error) => {
            // async_openai passes network errors through as reqwest errors.
            LanguageModelError::transient(error)
        }
        OpenAIError::JSONDeserialize(error, content) => {
            if let Some(provider_error) = ProviderError::from_response(&content) {
                map_classification(provider_error.classification(), provider_error)
            } else {
                // Retain the existing transient fallback for malformed success responses.
                LanguageModelError::transient(OpenAIError::JSONDeserialize(error, content))
            }
        }
        OpenAIError::StreamError(stream_error) => {
            LanguageModelError::permanent(OpenAIError::StreamError(stream_error))
        }
        OpenAIError::FileSaveError(_)
        | OpenAIError::FileReadError(_)
        | OpenAIError::InvalidArgument(_) => LanguageModelError::permanent(e),
    }
}

fn status_from_code(code: Option<&str>) -> Option<StatusCode> {
    code.and_then(|code| code.parse::<u16>().ok())
        .and_then(|code| StatusCode::from_u16(code).ok())
}

// Classification combines OpenRouter's canonical in-band error taxonomy with OpenAI's HTTP error
// behavior, in precedence order:
//
// 1. `context_length_exceeded` maps to Swiftide's specialized context-length error.
// 2. OpenAI's documented billing/spend/quota identifiers override HTTP 429 because retrying them
//    cannot succeed without account action. `OPENAI_NON_RETRYABLE_QUOTA_ERRORS` contains the full
//    set currently documented at:
//    https://developers.openai.com/api/docs/guides/error-codes#api-errors
// 3. `OPENROUTER_TRANSIENT_ERROR_TYPES` is built from every canonical `error_type` in OpenRouter's
//    rate-limiting and availability group (`rate_limit_exceeded`, `provider_overloaded`, and
//    `provider_unavailable`) plus its retryable generic failures (`server` and `timeout`):
//    https://openrouter.ai/docs/api_reference/errors-and-debugging#typed-error-codes
// 4. OpenRouter documents an in-band numeric `error.code` as the equivalent HTTP status, so 408,
//    429, and 5xx are transient just like ordinary HTTP errors.
// 5. All remaining documented OpenRouter types (other token/length, authentication/authorization,
//    request validation, content policy, image, and `unmapped`) default to permanent. OpenRouter
//    transforms the other token/length types into successful Chat Completions with finish_reason
//    `length`, so only `context_length_exceeded` needs special error handling here.
fn classify_error<'a>(
    status: Option<StatusCode>,
    identifiers: impl IntoIterator<Item = Option<&'a str>>,
) -> ErrorClassification {
    let identifiers = identifiers.into_iter().flatten().collect::<Vec<_>>();

    if contains_identifier(&identifiers, &["context_length_exceeded"]) {
        return ErrorClassification::ContextLengthExceeded;
    }

    // Explicit non-retryable types take precedence over a transport status.
    if contains_identifier(&identifiers, OPENAI_NON_RETRYABLE_QUOTA_ERRORS) {
        return ErrorClassification::Permanent;
    }

    if contains_identifier(&identifiers, OPENROUTER_TRANSIENT_ERROR_TYPES)
        || status.is_some_and(|status| {
            status == StatusCode::REQUEST_TIMEOUT
                || status == StatusCode::TOO_MANY_REQUESTS
                || status.is_server_error()
        })
    {
        ErrorClassification::Transient
    } else {
        ErrorClassification::Permanent
    }
}

fn contains_identifier(identifiers: &[&str], expected: &[&str]) -> bool {
    identifiers.iter().any(|identifier| {
        expected
            .iter()
            .any(|expected| identifier.eq_ignore_ascii_case(expected))
    })
}

fn map_classification(
    classification: ErrorClassification,
    error: impl Into<Box<dyn std::error::Error + Send + Sync>>,
) -> LanguageModelError {
    match classification {
        ErrorClassification::ContextLengthExceeded => {
            LanguageModelError::context_length_exceeded(error)
        }
        ErrorClassification::Permanent => LanguageModelError::permanent(error),
        ErrorClassification::Transient => LanguageModelError::transient(error),
    }
}

#[cfg(test)]
mod tests {
    use async_openai::error::{ApiError, ApiErrorResponse};

    use super::*;

    fn json_deserialize_error(content: &str) -> OpenAIError {
        let error = serde_json::from_str::<String>("{").unwrap_err();
        OpenAIError::JSONDeserialize(error, content.to_owned())
    }

    fn api_error(status_code: StatusCode, error_type: &str, code: Option<&str>) -> OpenAIError {
        OpenAIError::ApiError(ApiErrorResponse {
            status_code,
            api_error: ApiError {
                message: "provider message".to_owned(),
                r#type: Some(error_type.to_owned()),
                param: None,
                code: code.map(str::to_owned),
            },
        })
    }

    #[test]
    fn numeric_code_provider_error_is_transient_and_preserves_details() {
        let error = json_deserialize_error(
            r#"{"error":{"message":"Service unavailable.","code":504,"metadata":{"error_type":"timeout"}}}"#,
        );

        let result = openai_error_to_language_model_error(error);
        assert!(matches!(result, LanguageModelError::TransientError(_)));
        let message = result.to_string();
        assert!(message.contains("Service unavailable."));
        assert!(message.contains("504"));
        assert!(message.contains("timeout"));
        assert!(!message.contains("missing field"));
    }

    #[test]
    fn string_rate_limit_code_is_transient() {
        let error = json_deserialize_error(
            r#"{"error":{"message":"Slow down","code":"429","metadata":{"error_type":"rate_limit_exceeded"}}}"#,
        );

        assert!(matches!(
            openai_error_to_language_model_error(error),
            LanguageModelError::TransientError(_)
        ));
    }

    #[test]
    fn partial_completion_provider_error_is_recognized() {
        let error = json_deserialize_error(
            r#"{"choices":[{"finish_reason":"error","error":{"message":"Provider disconnected","code":502,"metadata":{"error_type":"provider_unavailable"}}}]}"#,
        );

        let result = openai_error_to_language_model_error(error);
        assert!(matches!(result, LanguageModelError::TransientError(_)));
        assert!(result.to_string().contains("Provider disconnected"));
    }

    #[test]
    fn openai_quota_errors_are_permanent_even_with_rate_limit_status() {
        for error in [
            json_deserialize_error(
                r#"{"error":{"message":"Buy credits","code":429,"type":"insufficient_quota"}}"#,
            ),
            api_error(StatusCode::TOO_MANY_REQUESTS, "insufficient_quota", None),
        ] {
            assert!(matches!(
                openai_error_to_language_model_error(error),
                LanguageModelError::PermanentError(_)
            ));
        }

        for code in [
            "credit_balance_exhausted",
            "organization_spend_limit_exceeded",
            "project_spend_limit_exceeded",
            "organization_usage_limit_exceeded",
        ] {
            let error = api_error(StatusCode::TOO_MANY_REQUESTS, "billing_error", Some(code));
            assert!(matches!(
                openai_error_to_language_model_error(error),
                LanguageModelError::PermanentError(_)
            ));
        }
    }

    #[test]
    fn openrouter_availability_error_types_are_transient_without_status() {
        for error_type in [
            "rate_limit_exceeded",
            "provider_overloaded",
            "provider_unavailable",
            "server",
            "timeout",
        ] {
            let content = format!(
                r#"{{"error":{{"message":"Try again","metadata":{{"error_type":"{error_type}"}}}}}}"#
            );
            assert!(matches!(
                openai_error_to_language_model_error(json_deserialize_error(&content)),
                LanguageModelError::TransientError(_)
            ));
        }
    }

    #[test]
    fn context_length_error_from_envelope_is_specialized() {
        let error = json_deserialize_error(
            r#"{"error":{"message":"Too long","code":"context_length_exceeded"}}"#,
        );

        assert!(matches!(
            openai_error_to_language_model_error(error),
            LanguageModelError::ContextLengthExceeded(_)
        ));
    }

    #[test]
    fn exhausted_server_api_error_is_transient() {
        let error = api_error(StatusCode::BAD_GATEWAY, "api_error", None);

        assert!(matches!(
            openai_error_to_language_model_error(error),
            LanguageModelError::TransientError(_)
        ));
    }

    #[test]
    fn non_retryable_http_errors_are_permanent() {
        for status in [
            StatusCode::UNAUTHORIZED,
            StatusCode::PAYMENT_REQUIRED,
            StatusCode::FORBIDDEN,
            StatusCode::UNPROCESSABLE_ENTITY,
        ] {
            let error = api_error(status, "request_error", None);
            assert!(matches!(
                openai_error_to_language_model_error(error),
                LanguageModelError::PermanentError(_)
            ));
        }
    }

    #[test]
    fn malformed_response_fallback_is_transient() {
        let content = "not a chat completion";
        let result = openai_error_to_language_model_error(json_deserialize_error(content));

        assert!(matches!(result, LanguageModelError::TransientError(_)));
        assert!(result.to_string().contains(content));
    }
}
