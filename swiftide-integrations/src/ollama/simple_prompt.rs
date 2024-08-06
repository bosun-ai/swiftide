//! This module provides an implementation of the `SimplePrompt` trait for the `Ollama` struct.
//! It defines an asynchronous function to interact with the `Ollama` API, allowing prompt processing
//! and generating responses as part of the Swiftide system.
use async_trait::async_trait;
use swiftide_core::{prompt::Prompt, SimplePrompt};

use super::Ollama;
use anyhow::{Context as _, Result};

/// The `SimplePrompt` trait defines a method for sending a prompt to an AI model and receiving a response.
#[async_trait]
impl SimplePrompt for Ollama {
    /// Sends a prompt to the Ollama API and returns the response content.
    ///
    /// # Parameters
    /// - `prompt`: A string slice that holds the prompt to be sent to the Ollama API.
    ///
    /// # Returns
    /// - `Result<String>`: On success, returns the content of the response as a `String`.
    ///   On failure, returns an error wrapped in a `Result`.
    ///
    /// # Errors
    /// - Returns an error if the model is not set in the default options.
    /// - Returns an error if the request to the Ollama API fails.
    /// - Returns an error if the response does not contain the expected content.
    #[tracing::instrument(skip_all, err)]
    async fn prompt(&self, prompt: Prompt) -> Result<String> {
        // Retrieve the model from the default options, returning an error if not set.
        let model = self
            .default_options
            .prompt_model
            .as_ref()
            .context("Model not set")?;

        // Build the request to be sent to the Ollama API.
        let request = ollama_rs::generation::completion::request::GenerationRequest::new(
            model.to_string(),
            prompt.render().await?,
        );

        // Log the request for debugging purposes.
        tracing::debug!(
            messages = serde_json::to_string_pretty(&request)?,
            "[SimplePrompt] Request to ollama"
        );

        // Send the request to the Ollama API and await the response.
        // let mut response = self.client.chat().create(request).await?;
        let response = self.client.generate(request).await?;

        // Log the response for debugging purposes.
        tracing::debug!(
            response = serde_json::to_string_pretty(&response.response)?,
            "[SimplePrompt] Response from ollama"
        );

        // Extract and return the content of the response, returning an error if not found.
        Ok(response.response)
    }
}
