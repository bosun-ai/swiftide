//! This module provides an implementation of the `SimplePrompt` trait for the `OpenAI` struct.
//! It defines an asynchronous function to interact with the `OpenAI` API, allowing prompt processing
//! and generating responses as part of the Swiftide system.
use async_openai::{error::OpenAIError, types::{ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs}};
use async_trait::async_trait;
use swiftide_core::{prompt::Prompt, util::debug_long_utf8, PromptError, SimplePrompt};

use super::OpenAI;
use anyhow::{anyhow, Context as _, Result};

/// The `SimplePrompt` trait defines a method for sending a prompt to an AI model and receiving a response.
#[async_trait]
impl<C: async_openai::config::Config + std::default::Default + Sync + Send + std::fmt::Debug>
    SimplePrompt for OpenAI<C>
{
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
    async fn prompt(&self, prompt: Prompt) -> Result<String, PromptError> {
        // Retrieve the model from the default options, returning an error if not set.
        let model = self
            .default_options
            .prompt_model
            .as_ref()
            .context("Model not set")
            .map_err(PromptError::ClientError)?;

        // Build the request to be sent to the OpenAI API.
        let request = CreateChatCompletionRequestArgs::default()
            .model(model)
            .messages(vec![ChatCompletionRequestUserMessageArgs::default()
                .content(prompt.render().await.map_err(PromptError::ClientError)?)
                .build().map_err(|e| PromptError::ClientError(e.into()))?
                .into()])
            .build().map_err(|e| PromptError::ClientError(e.into()))?;

        // Log the request for debugging purposes.
        tracing::debug!(
            model = &model,
            messages = debug_long_utf8(
                serde_json::to_string_pretty(&request.messages.first())
                .map_err(|e| PromptError::ClientError(e.into()))?,
                100
            ),
            "[SimplePrompt] Request to openai"
        );

        // Send the request to the OpenAI API and await the response.
        let response = self
            .client
            .chat()
            .create(request)
            .await;

        let mut response = response.map_err(|e| match e {
            OpenAIError::ApiError(api_error) => {
                // If the response is an ApiError, it could be a context length exceeded error
                if api_error.code == Some("context_length_exceeded".to_string()) {
                    PromptError::ContextLengthExceeded(anyhow!(api_error))
                } else {
                    tracing::error!("OpenAI API Error: {:?}", api_error);
                    PromptError::ClientError(anyhow!(api_error))
                }
            },
            OpenAIError::Reqwest(e) => match e.status() {
                Some(status) => {
                    // If the response code is 429 it could either be a TransientError or a ClientError depending
                    // on the message, if it contains the word quota, it should be a ClientError otherwise it should
                    // be a TransientError.
                    // If the response code is any other 4xx it should be a ClientError.
                    if status.as_u16() == 429 && !e.to_string().contains("quota") {
                        PromptError::TransientError(e.into())
                    } else if status.is_client_error() {
                        tracing::error!("OpenAI API Client Error: {:?}", e);
                        PromptError::ClientError(e.into())
                    } else if status.is_server_error() {
                        tracing::warn!("OpenAI API Server Error: {:?}", e);
                        PromptError::TransientError(e.into())
                    } else {
                        tracing::error!("Unexpected OpenAI Error: {:?}, error: {:?}", status, e);
                        PromptError::ClientError(e.into())
                    }
                },
                _ => {
                    // making the request failed for some other reason, probably recoverable
                    tracing::error!("Unexpected OpenAI Reqwest Error: {:?}", e);
                    PromptError::TransientError(e.into())
                },
            }
            OpenAIError::JSONDeserialize(e) => {
                // OpenAI generated a non-json response, probably a temporary problem on their side
                tracing::error!("OpenAI response could not be deserialized: {:?}", e);
                PromptError::TransientError(e.into())
            },
            OpenAIError::FileSaveError(msg) => {
                tracing::error!("OpenAI Failed to save file: {:?}", msg);
                PromptError::ClientError(anyhow!(msg))
            },
            OpenAIError::FileReadError(msg) => {
                tracing::error!("OpenAI Failed to read file: {:?}", msg);
                PromptError::ClientError(anyhow!(msg))
            },
            OpenAIError::StreamError(msg) => {
                tracing::error!("OpenAI Stream failed: {:?}", msg);
                PromptError::ClientError(anyhow!(msg))
            },
            OpenAIError::InvalidArgument(msg) => {
                tracing::error!("OpenAI Invalid Argument: {:?}", msg);
                PromptError::ClientError(anyhow!(msg))
            },
        })?;
        
        let response = response
            .choices
            .remove(0)
            .message
            .content
            .take()
            .context("Expected content in response")
            .map_err(PromptError::ClientError)?;

        // Log the response for debugging purposes.
        tracing::debug!(
            response = debug_long_utf8(&response, 100),
            "[SimplePrompt] Response from openai"
        );

        // Extract and return the content of the response, returning an error if not found.
        Ok(response)
    }
}
