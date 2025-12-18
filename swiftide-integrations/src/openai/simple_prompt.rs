//! This module provides an implementation of the `SimplePrompt` trait for the `OpenAI` struct.
//! It defines an asynchronous function to interact with the `OpenAI` API, allowing prompt
//! processing and generating responses as part of the Swiftide system.

use async_openai::types::chat::ChatCompletionRequestUserMessageArgs;
use async_trait::async_trait;
use swiftide_core::{
    SimplePrompt, chat_completion::errors::LanguageModelError, prompt::Prompt,
    util::debug_long_utf8,
};

use super::chat_completion::usage_from_counts;
use super::responses_api::{build_responses_request_from_prompt, response_to_chat_completion};
use crate::openai::openai_error_to_language_model_error;

use super::GenericOpenAI;
use anyhow::Result;

/// The `SimplePrompt` trait defines a method for sending a prompt to an AI model and receiving a
/// response.
#[async_trait]
impl<
    C: async_openai::config::Config
        + std::default::Default
        + Sync
        + Send
        + std::fmt::Debug
        + Clone
        + 'static,
> SimplePrompt for GenericOpenAI<C>
{
    /// Sends a prompt to the `OpenAI` API and returns the response content.
    ///
    /// # Parameters
    /// - `prompt`: A string slice that holds the prompt to be sent to the `OpenAI` API.
    ///
    /// # Returns
    /// - `Result<String>`: On success, returns the content of the response as a `String`. On
    ///   failure, returns an error wrapped in a `Result`.
    ///
    /// # Errors
    /// - Returns an error if the model is not set in the default options.
    /// - Returns an error if the request to the `OpenAI` API fails.
    /// - Returns an error if the response does not contain the expected content.
    #[cfg_attr(not(feature = "langfuse"), tracing::instrument(skip_all, err))]
    #[cfg_attr(
        feature = "langfuse",
        tracing::instrument(skip_all, err, fields(langfuse.type = "GENERATION"))
    )]
    async fn prompt(&self, prompt: Prompt) -> Result<String, LanguageModelError> {
        if self.is_responses_api_enabled() {
            return self.prompt_via_responses_api(prompt).await;
        }

        // Retrieve the model from the default options, returning an error if not set.
        let model = self
            .default_options
            .prompt_model
            .as_ref()
            .ok_or_else(|| LanguageModelError::PermanentError("Model not set".into()))?;

        // Build the request to be sent to the OpenAI API.
        let request = self
            .chat_completion_request_defaults()
            .model(model)
            .messages(vec![
                ChatCompletionRequestUserMessageArgs::default()
                    .content(prompt.render()?)
                    .build()
                    .map_err(LanguageModelError::permanent)?
                    .into(),
            ])
            .build()
            .map_err(LanguageModelError::permanent)?;

        // Log the request for debugging purposes.
        tracing::trace!(
            model = &model,
            messages = debug_long_utf8(
                serde_json::to_string_pretty(&request.messages.last())
                    .map_err(LanguageModelError::permanent)?,
                100
            ),
            "[SimplePrompt] Request to openai"
        );

        // Send the request to the OpenAI API and await the response.
        // Move the request; we logged key fields above if needed.
        let tracking_request = request.clone();
        let response = self
            .client
            .chat()
            .create(request)
            .await
            .map_err(openai_error_to_language_model_error)?;

        let message = response
            .choices
            .first()
            .and_then(|choice| choice.message.content.clone())
            .ok_or_else(|| {
                LanguageModelError::PermanentError("Expected content in response".into())
            })?;

        let usage = response.usage.as_ref().map(|usage| {
            usage_from_counts(
                usage.prompt_tokens,
                usage.completion_tokens,
                usage.total_tokens,
            )
        });

        self.track_completion(
            model,
            usage.as_ref(),
            Some(&tracking_request),
            Some(&response),
        );

        Ok(message)
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
    async fn prompt_via_responses_api(&self, prompt: Prompt) -> Result<String, LanguageModelError> {
        let prompt_text = prompt.render().map_err(LanguageModelError::permanent)?;
        let model = self
            .default_options
            .prompt_model
            .as_ref()
            .ok_or_else(|| LanguageModelError::PermanentError("Model not set".into()))?;

        let create_request = build_responses_request_from_prompt(self, prompt_text.clone())?;

        let response = self
            .client
            .responses()
            .create(create_request.clone())
            .await
            .map_err(openai_error_to_language_model_error)?;

        let completion = response_to_chat_completion(&response)?;

        let message = completion.message.clone().ok_or_else(|| {
            LanguageModelError::PermanentError("Expected content in response".into())
        })?;

        self.track_completion(
            model,
            completion.usage.as_ref(),
            Some(&create_request),
            Some(&completion),
        );

        Ok(message)
    }
}

#[allow(clippy::items_after_statements)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::OpenAI;
    use serde_json::Value;
    use wiremock::{
        Mock, MockServer, Request, Respond, ResponseTemplate,
        matchers::{method, path},
    };

    #[test_log::test(tokio::test)]
    async fn test_prompt_errors_when_model_missing() {
        let openai = OpenAI::builder().build().unwrap();
        let result = openai.prompt("hello".into()).await;
        assert!(matches!(result, Err(LanguageModelError::PermanentError(_))));
    }

    #[test_log::test(tokio::test)]
    async fn test_prompt_via_responses_api_returns_message() {
        let mock_server = MockServer::start().await;

        let response_body = serde_json::json!({
            "created_at": 0,
            "id": "resp",
            "model": "gpt-4.1-mini",
            "object": "response",
            "status": "completed",
            "output": [
                {
                    "type": "message",
                    "id": "msg",
                    "role": "assistant",
                    "status": "completed",
                    "content": [
                        {"type": "output_text", "text": "Hello world", "annotations": []}
                    ]
                }
            ],
            "usage": {
                "input_tokens": 4,
                "input_tokens_details": {"cached_tokens": 0},
                "output_tokens": 2,
                "output_tokens_details": {"reasoning_tokens": 0},
                "total_tokens": 6
            }
        });

        struct ValidatePromptRequest {
            response: Value,
        }

        impl Respond for ValidatePromptRequest {
            fn respond(&self, request: &Request) -> ResponseTemplate {
                let payload: Value = serde_json::from_slice(&request.body).unwrap();
                assert_eq!(payload["model"], self.response["model"]);
                let items = payload["input"].as_array().expect("array input");
                assert_eq!(items.len(), 1);
                assert_eq!(items[0]["type"], "message");
                ResponseTemplate::new(200).set_body_json(self.response.clone())
            }
        }

        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ValidatePromptRequest {
                response: response_body,
            })
            .mount(&mock_server)
            .await;

        let config = async_openai::config::OpenAIConfig::new().with_api_base(mock_server.uri());
        let client = async_openai::Client::with_config(config);

        let openai = OpenAI::builder()
            .client(client)
            .default_prompt_model("gpt-4.1-mini")
            .use_responses_api(true)
            .build()
            .unwrap();

        let result = openai.prompt("Say hi".into()).await.unwrap();
        assert_eq!(result, "Hello world");
    }

    #[test_log::test(tokio::test)]
    async fn test_prompt_via_responses_api_missing_output_errors() {
        let mock_server = MockServer::start().await;
        let empty_response = serde_json::json!({
            "created_at": 0,
            "id": "resp",
            "model": "gpt-4.1-mini",
            "object": "response",
            "output": [],
            "status": "completed"
        });

        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(200).set_body_json(empty_response))
            .mount(&mock_server)
            .await;

        let config = async_openai::config::OpenAIConfig::new().with_api_base(mock_server.uri());
        let client = async_openai::Client::with_config(config);

        let openai = OpenAI::builder()
            .client(client)
            .default_prompt_model("gpt-4.1-mini")
            .use_responses_api(true)
            .build()
            .unwrap();

        let err = openai.prompt("test".into()).await.unwrap_err();
        assert!(matches!(err, LanguageModelError::PermanentError(_)));
    }
}
