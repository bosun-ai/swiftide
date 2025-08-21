//! This module provides an implementation of the `SimplePrompt` trait for the `OpenAI` struct.
//! It defines an asynchronous function to interact with the `OpenAI` API, allowing prompt
//! processing and generating responses as part of the Swiftide system.

use async_openai::types::ChatCompletionRequestUserMessageArgs;
use async_trait::async_trait;
#[cfg(feature = "metrics")]
use swiftide_core::metrics::emit_usage;
use swiftide_core::{
    SimplePrompt, chat_completion::errors::LanguageModelError, prompt::Prompt,
    util::debug_long_utf8,
};

use crate::openai::openai_error_to_language_model_error;

use super::GenericOpenAI;
use anyhow::Result;

/// The `SimplePrompt` trait defines a method for sending a prompt to an AI model and receiving a
/// response.
#[async_trait]
impl<
    C: async_openai::config::Config + std::default::Default + Sync + Send + std::fmt::Debug + Clone,
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
    #[tracing::instrument(skip_all, err)]
    #[cfg_attr(
        feature = "langfuse",
        tracing::instrument(skip_all, err, langfuse.type = "GENERATION")
    )]
    async fn prompt(&self, prompt: Prompt) -> Result<String, LanguageModelError> {
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

        #[cfg(feature = "metrics")]
        {
            if let Some(usage) = response.usage.as_ref() {
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

        // Emit Langfuse event with the response details.

        // Extract and return the content of the response, returning an error if not found.
        Ok(message)
    }
}
