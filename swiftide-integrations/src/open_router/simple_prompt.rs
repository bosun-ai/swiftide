//! This module provides an implementation of the `SimplePrompt` trait for the `OpenRouter` struct.
//! It defines an asynchronous function to interact with the `OpenRouter` API, allowing prompt processing
//! and generating responses as part of the Swiftide system.
use async_openai::types::{ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs};
use async_trait::async_trait;
use swiftide_core::{prompt::Prompt, util::debug_long_utf8, SimplePrompt};

use super::OpenRouter;
use anyhow::{Context as _, Result};

/// The `SimplePrompt` trait defines a method for sending a prompt to an AI model and receiving a response.
#[async_trait]
impl SimplePrompt for OpenRouter {
    /// Sends a prompt to the OpenRouter API and returns the response content.
    ///
    /// # Parameters
    /// - `prompt`: A string slice that holds the prompt to be sent to the OpenRouter API.
    ///
    /// # Returns
    /// - `Result<String>`: On success, returns the content of the response as a `String`.
    ///   On failure, returns an error wrapped in a `Result`.
    ///
    /// # Errors
    /// - Returns an error if the model is not set in the default options.
    /// - Returns an error if the request to the OpenRouter API fails.
    /// - Returns an error if the response does not contain the expected content.
    #[tracing::instrument(skip_all, err)]
    async fn prompt(&self, prompt: Prompt) -> Result<String> {
        // Retrieve the model from the default options, returning an error if not set.
        let model = self
            .default_options
            .prompt_model
            .as_ref()
            .context("Model not set")?;

        // Build the request to be sent to the OpenRouter API.
        let request = CreateChatCompletionRequestArgs::default()
            .model(model)
            .messages(vec![ChatCompletionRequestUserMessageArgs::default()
                .content(prompt.render().await?)
                .build()?
                .into()])
            .build()?;

        // Log the request for debugging purposes.
        tracing::debug!(
            model = &model,
            messages = debug_long_utf8(
                serde_json::to_string_pretty(&request.messages.first())?,
                100
            ),
            "[SimplePrompt] Request to openrouter"
        );

        // Send the request to the OpenRouter API and await the response.
        let response = self
            .client
            .chat()
            .create(request)
            .await?
            .choices
            .remove(0)
            .message
            .content
            .take()
            .context("Expected content in response")?;

        // Log the response for debugging purposes.
        tracing::debug!(
            response = debug_long_utf8(&response, 100),
            "[SimplePrompt] Response from openrouter"
        );

        // Extract and return the content of the response, returning an error if not found.
        Ok(response)
    }
}
