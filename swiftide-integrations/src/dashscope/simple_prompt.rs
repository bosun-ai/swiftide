use async_openai::types::{ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs};
use async_trait::async_trait;
use swiftide_core::{chat_completion::errors::ChatCompletionError, prompt::Prompt, SimplePrompt};

use crate::openai::open_ai_error_to_completion_error;

use super::Dashscope;
use anyhow::{Context as _, Result};

#[async_trait]
impl SimplePrompt for Dashscope {
    async fn prompt(&self, prompt: Prompt) -> Result<String, ChatCompletionError> {
        let model = self
            .default_options
            .prompt_model
            .as_ref()
            .context("Model not set")?
            .to_string();

        let request = CreateChatCompletionRequestArgs::default()
            .model(model)
            .messages(vec![ChatCompletionRequestUserMessageArgs::default()
                .content(prompt.render().await?)
                .build()
                .map_err(open_ai_error_to_completion_error)?
                .into()])
            .build()
            .map_err(open_ai_error_to_completion_error)?;

        tracing::debug!(
            messages = serde_json::to_string_pretty(&request)
                .map_err(|e| ChatCompletionError::ClientError(e.into()))?,
            "[SimplePrompt] Request to qwen"
        );

        let mut response = self
            .client
            .chat()
            .create(request)
            .await
            .map_err(open_ai_error_to_completion_error)?;

        tracing::debug!(
            response = serde_json::to_string_pretty(&response)
                .map_err(|e| ChatCompletionError::ClientError(e.into()))?,
            "[SimplePrompt] Response from qwen"
        );

        response
            .choices
            .remove(0)
            .message
            .content
            .take()
            .ok_or(ChatCompletionError::ClientError(
                "Expected content in response".into(),
            ))
    }
}
