//! This module provides an implementation of the `StructuredPrompt` trait for the `OpenAI` struct.
//!
//! Unlike the other traits, `StructuredPrompt` is *not* dyn safe.
//!
//! Use `DynStructuredPrompt` if you need dyn dispatch. For custom implementations, if you
//! implement `DynStructuredPrompt`, you get `StructuredPrompt` for free.

use async_openai::types::{
    ChatCompletionRequestUserMessageArgs, ResponseFormat, ResponseFormatJsonSchema,
};
use async_trait::async_trait;
use schemars::Schema;
#[cfg(feature = "metrics")]
use swiftide_core::metrics::emit_usage;
use swiftide_core::{
    DynStructuredPrompt,
    chat_completion::{Usage, errors::LanguageModelError},
    prompt::Prompt,
    util::debug_long_utf8,
};

use crate::openai::openai_error_to_language_model_error;

use super::GenericOpenAI;
use anyhow::{Context as _, Result};

/// The `StructuredPrompt` trait defines a method for sending a prompt to an AI model and receiving
/// a response.
#[async_trait]
impl<
    C: async_openai::config::Config + std::default::Default + Sync + Send + std::fmt::Debug + Clone,
> DynStructuredPrompt for GenericOpenAI<C>
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
    #[tracing::instrument(skip_all, err)]
    #[cfg_attr(
        feature = "langfuse",
        tracing::instrument(skip_all, err, fields(langfuse.type = "GENERATION"))
    )]
    async fn structured_prompt_dyn(
        &self,
        prompt: Prompt,
        schema: Schema,
    ) -> Result<serde_json::Value, LanguageModelError> {
        // Retrieve the model from the default options, returning an error if not set.
        let model = self
            .default_options
            .prompt_model
            .as_ref()
            .ok_or_else(|| LanguageModelError::PermanentError("Model not set".into()))?;

        let schema_value =
            serde_json::to_value(&schema).context("Failed to get schema as value")?;
        let response_format = ResponseFormat::JsonSchema {
            json_schema: ResponseFormatJsonSchema {
                description: None,
                name: "math_reasoning".into(),
                schema: Some(schema_value),
                strict: Some(true),
            },
        };

        // Build the request to be sent to the OpenAI API.
        let request = self
            .chat_completion_request_defaults()
            .model(model)
            .response_format(response_format)
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
            "[StructuredPrompt] Request to openai"
        );

        // Send the request to the OpenAI API and await the response.
        let mut response = self
            .client
            .chat()
            .create(request.clone())
            .await
            .map_err(openai_error_to_language_model_error)?;

        if cfg!(feature = "langfuse") {
            let usage = response.usage.clone().unwrap_or_default();
            tracing::debug!(
                langfuse.model = model,
                langfuse.input = %serde_json::to_string_pretty(&request).unwrap_or_default(),
                langfuse.output = %serde_json::to_string_pretty(&response).unwrap_or_default(),
                langfuse.usage = %serde_json::to_string_pretty(&usage).unwrap_or_default(),
            );
        }

        let message = response
            .choices
            .remove(0)
            .message
            .content
            .take()
            .ok_or_else(|| {
                LanguageModelError::PermanentError("Expected content in response".into())
            })?;

        {
            if let Some(usage) = response.usage.as_ref() {
                if let Some(callback) = &self.on_usage {
                    let usage = Usage {
                        prompt_tokens: usage.prompt_tokens,
                        completion_tokens: usage.completion_tokens,
                        total_tokens: usage.total_tokens,
                    };
                    callback(&usage).await?;
                }
                #[cfg(feature = "metrics")]
                emit_usage(
                    model,
                    usage.prompt_tokens.into(),
                    usage.completion_tokens.into(),
                    usage.total_tokens.into(),
                    self.metric_metadata.as_ref(),
                );
            } else {
                tracing::warn!("Metrics enabled but no usage data found in response");
            }
        }

        let parsed = serde_json::from_str(&message)
            .with_context(|| format!("Failed to parse response\n {message}"))?;

        // Extract and return the content of the response, returning an error if not found.
        Ok(parsed)
    }
}

#[cfg(test)]
mod tests {
    use crate::openai::{self, OpenAI};
    use swiftide_core::StructuredPrompt;

    use super::*;
    use async_openai::Client;
    use async_openai::config::OpenAIConfig;
    use schemars::{JsonSchema, schema_for};
    use serde::{Deserialize, Serialize};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
    struct SimpleOutput {
        answer: String,
    }

    async fn setup_client() -> (MockServer, OpenAI) {
        // Start the Wiremock server
        let mock_server = MockServer::start().await;

        // Prepare the response the mock should return
        let assistant_msg = serde_json::json!({
            "role": "assistant",
            "content": serde_json::to_string(&SimpleOutput {
                answer: "42".to_owned()
            }).unwrap(),
        });

        let body = serde_json::json!({
          "id": "chatcmpl-B9MBs8CjcvOU2jLn4n570S5qMJKcT",
          "object": "chat.completion",
          "created": 123,
          "model": "gpt-4.1-2025-04-14",
          "choices": [
            {
              "index": 0,
              "message": assistant_msg,
              "logprobs": null,
              "finish_reason": "stop"
            }
          ],
          "usage": {
            "prompt_tokens": 19,
            "completion_tokens": 10,
            "total_tokens": 29,
            "prompt_tokens_details": {
              "cached_tokens": 0,
              "audio_tokens": 0
            },
            "completion_tokens_details": {
              "reasoning_tokens": 0,
              "audio_tokens": 0,
              "accepted_prediction_tokens": 0,
              "rejected_prediction_tokens": 0
            }
          },
          "service_tier": "default"
        });

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&mock_server)
            .await;

        // Point our client at the mock server
        let config = OpenAIConfig::new().with_api_base(mock_server.uri());
        let client = Client::with_config(config);

        // Construct the GenericOpenAI instance
        let opts = openai::Options {
            prompt_model: Some("gpt-4".to_string()),
            ..openai::Options::default()
        };
        (
            mock_server,
            OpenAI::builder()
                .client(client)
                .default_options(opts)
                .build()
                .unwrap(),
        )
    }

    #[tokio::test]
    async fn test_structured_prompt_with_wiremock() {
        let (_guard, ai) = setup_client().await;
        // Call structured_prompt
        let result: serde_json::Value = ai.structured_prompt("test".into()).await.unwrap();
        dbg!(&result);

        // Assert
        assert_eq!(
            serde_json::from_value::<SimpleOutput>(result).unwrap(),
            SimpleOutput {
                answer: "42".into()
            }
        );
    }

    #[tokio::test]
    async fn test_structured_prompt_with_wiremock_as_box() {
        let (_guard, ai) = setup_client().await;
        // Call structured_prompt
        let ai: Box<dyn DynStructuredPrompt> = Box::new(ai);
        let result: serde_json::Value = ai
            .structured_prompt_dyn("test".into(), schema_for!(SimpleOutput))
            .await
            .unwrap();
        dbg!(&result);

        // Assert
        assert_eq!(
            serde_json::from_value::<SimpleOutput>(result).unwrap(),
            SimpleOutput {
                answer: "42".into()
            }
        );
    }
}
