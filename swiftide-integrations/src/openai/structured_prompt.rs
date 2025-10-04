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
use swiftide_core::{
    DynStructuredPrompt, chat_completion::errors::LanguageModelError, prompt::Prompt,
    util::debug_long_utf8,
};

use super::chat_completion::{langfuse_json, usage_from_counts};
use super::responses_api::{
    build_responses_request_from_prompt_with_schema, response_to_chat_completion,
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
        if self.is_responses_api_enabled() {
            return self
                .structured_prompt_via_responses_api(prompt, schema)
                .await;
        }

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
                name: "structured_prompt".into(),
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
        let response = self
            .client
            .chat()
            .create(request.clone())
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

        let request_json = langfuse_json(&request);
        let response_json = langfuse_json(&response);
        let usage_json = usage.as_ref().and_then(langfuse_json);

        self.track_completion(
            model,
            usage.as_ref(),
            request_json.as_deref(),
            response_json.as_deref(),
            usage_json.as_deref(),
        )
        .await?;

        let parsed = serde_json::from_str(&message)
            .with_context(|| format!("Failed to parse response\n {message}"))?;

        // Extract and return the content of the response, returning an error if not found.
        Ok(parsed)
    }
}

impl<
    C: async_openai::config::Config + std::default::Default + Sync + Send + std::fmt::Debug + Clone,
> GenericOpenAI<C>
{
    async fn structured_prompt_via_responses_api(
        &self,
        prompt: Prompt,
        schema: Schema,
    ) -> Result<serde_json::Value, LanguageModelError> {
        let prompt_text = prompt.render().map_err(LanguageModelError::permanent)?;
        let model = self
            .default_options
            .prompt_model
            .as_ref()
            .ok_or_else(|| LanguageModelError::PermanentError("Model not set".into()))?;

        let schema_value = serde_json::to_value(&schema)
            .context("Failed to get schema as value")
            .map_err(LanguageModelError::permanent)?;

        let create_request = build_responses_request_from_prompt_with_schema(
            self,
            prompt_text.clone(),
            schema_value,
        )?;
        let request_json = langfuse_json(&create_request);

        let response = self
            .client
            .responses()
            .create(create_request)
            .await
            .map_err(openai_error_to_language_model_error)?;

        let completion = response_to_chat_completion(&response)?;

        let usage_ref = completion.usage.as_ref();
        let response_json = langfuse_json(&completion);
        let usage_json = usage_ref.and_then(langfuse_json);

        let message = completion.message.clone().ok_or_else(|| {
            LanguageModelError::PermanentError("Expected content in response".into())
        })?;

        self.track_completion(
            model,
            usage_ref,
            request_json.as_deref(),
            response_json.as_deref(),
            usage_json.as_deref(),
        )
        .await?;

        let parsed = serde_json::from_str(&message)
            .with_context(|| format!("Failed to parse response\n {message}"))
            .map_err(LanguageModelError::permanent)?;

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
    use async_openai::types::responses::{
        CompletionTokensDetails, Content, OutputContent, OutputMessage, OutputStatus, OutputText,
        PromptTokensDetails, Response as ResponsesResponse, Role, Status, Usage as ResponsesUsage,
    };
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

    #[test_log::test(tokio::test)]
    async fn test_structured_prompt_via_responses_api() {
        let mock_server = MockServer::start().await;

        let response = ResponsesResponse {
            created_at: 0,
            error: None,
            id: "resp".into(),
            incomplete_details: None,
            instructions: None,
            max_output_tokens: None,
            metadata: None,
            model: "gpt-4.1-mini".into(),
            object: "response".into(),
            output: vec![OutputContent::Message(OutputMessage {
                content: vec![Content::OutputText(OutputText {
                    annotations: Vec::new(),
                    text: serde_json::to_string(&SimpleOutput {
                        answer: "structured".into(),
                    })
                    .unwrap(),
                })],
                id: "msg".into(),
                role: Role::Assistant,
                status: OutputStatus::Completed,
            })],
            output_text: None,
            parallel_tool_calls: None,
            previous_response_id: None,
            reasoning: None,
            store: None,
            service_tier: None,
            status: Status::Completed,
            temperature: None,
            text: None,
            tool_choice: None,
            tools: None,
            top_p: None,
            truncation: None,
            usage: Some(ResponsesUsage {
                input_tokens: 10,
                input_tokens_details: PromptTokensDetails {
                    audio_tokens: Some(0),
                    cached_tokens: Some(0),
                },
                output_tokens: 4,
                output_tokens_details: CompletionTokensDetails {
                    accepted_prediction_tokens: Some(0),
                    audio_tokens: Some(0),
                    reasoning_tokens: Some(0),
                    rejected_prediction_tokens: Some(0),
                },
                total_tokens: 14,
            }),
            user: None,
        };

        let response_body = serde_json::to_value(&response).unwrap();

        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
            .mount(&mock_server)
            .await;

        let config = OpenAIConfig::new().with_api_base(mock_server.uri());
        let client = Client::with_config(config);

        let openai = OpenAI::builder()
            .client(client)
            .default_prompt_model("gpt-4.1-mini")
            .use_responses_api(true)
            .build()
            .unwrap();

        let schema = schema_for!(SimpleOutput);
        let result = openai
            .structured_prompt_dyn("Render".into(), schema)
            .await
            .unwrap();

        assert_eq!(
            serde_json::from_value::<SimpleOutput>(result).unwrap(),
            SimpleOutput {
                answer: "structured".into(),
            }
        );
    }

    #[test_log::test(tokio::test)]
    async fn test_structured_prompt_via_responses_api_invalid_json_errors() {
        let mock_server = MockServer::start().await;

        let bad_response = ResponsesResponse {
            created_at: 0,
            error: None,
            id: "resp".into(),
            incomplete_details: None,
            instructions: None,
            max_output_tokens: None,
            metadata: None,
            model: "gpt-4.1-mini".into(),
            object: "response".into(),
            output: vec![OutputContent::Message(OutputMessage {
                content: vec![Content::OutputText(OutputText {
                    annotations: Vec::new(),
                    text: "not json".into(),
                })],
                id: "msg".into(),
                role: Role::Assistant,
                status: OutputStatus::Completed,
            })],
            output_text: Some("not json".into()),
            parallel_tool_calls: None,
            previous_response_id: None,
            reasoning: None,
            store: None,
            service_tier: None,
            status: Status::Completed,
            temperature: None,
            text: None,
            tool_choice: None,
            tools: None,
            top_p: None,
            truncation: None,
            usage: None,
            user: None,
        };

        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(200).set_body_json(bad_response))
            .mount(&mock_server)
            .await;

        let config = OpenAIConfig::new().with_api_base(mock_server.uri());
        let client = Client::with_config(config);

        let openai = OpenAI::builder()
            .client(client)
            .default_prompt_model("gpt-4.1-mini")
            .use_responses_api(true)
            .build()
            .unwrap();

        let schema = schema_for!(SimpleOutput);
        let err = openai
            .structured_prompt_dyn("Render".into(), schema)
            .await
            .unwrap_err();

        assert!(matches!(err, LanguageModelError::PermanentError(_)));
    }
}
