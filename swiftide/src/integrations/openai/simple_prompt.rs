//! This module provides an implementation of the `SimplePrompt` trait for the `OpenAI` struct.
//! It defines an asynchronous function to interact with the OpenAI API, allowing prompt processing
//! and generating responses as part of the Swiftide system.
use crate::SimplePrompt;
use async_openai::types::{ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs};
use async_trait::async_trait;

use super::OpenAI;
use anyhow::{Context as _, Result};

/// The `SimplePrompt` trait defines a method for sending a prompt to an AI model and receiving a response.
#[async_trait]
impl SimplePrompt for OpenAI {
    /// Sends a prompt to the OpenAI API and returns the response content.
    ///
    /// # Parameters
    /// - `prompt`: A string slice that holds the prompt to be sent to the OpenAI API.
    ///
    /// # Returns
    /// - `Result<String>`: On success, returns the content of the response as a `String`.
    ///   On failure, returns an error wrapped in a `Result`.
    ///
    /// # Errors
    /// - Returns an error if the model is not set in the default options.
    /// - Returns an error if the request to the OpenAI API fails.
    /// - Returns an error if the response does not contain the expected content.
    #[tracing::instrument(skip_all, err)]
    async fn prompt(&self, prompt: &str) -> Result<String> {
        // Retrieve the model from the default options, returning an error if not set.
        let model = self
            .default_options
            .prompt_model
            .as_ref()
            .context("Model not set")?;

        // Build the request to be sent to the OpenAI API.
        let request = CreateChatCompletionRequestArgs::default()
            .model(model)
            .messages(vec![ChatCompletionRequestUserMessageArgs::default()
                .content(prompt)
                .build()?
                .into()])
            .build()?;

        // Log the request for debugging purposes.
        tracing::debug!(
            messages = serde_json::to_string_pretty(&request)?,
            "[SimplePrompt] Request to openai"
        );

        // Send the request to the OpenAI API and await the response.
        let mut response = self.client.chat().create(request).await?;

        // Log the response for debugging purposes.
        tracing::debug!(
            response = serde_json::to_string_pretty(&response)?,
            "[SimplePrompt] Response from openai"
        );

        // Extract and return the content of the response, returning an error if not found.
        response
            .choices
            .remove(0)
            .message
            .content
            .take()
            .context("Expected content in response")
    }
}
