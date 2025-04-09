use async_openai::types::{ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs};
use async_trait::async_trait;
use swiftide_core::{chat_completion::errors::LanguageModelError, prompt::Prompt, SimplePrompt};

use crate::openai::openai_error_to_language_model_error;

use super::Dashscope;
use anyhow::{Context as _, Result};

#[async_trait]
impl SimplePrompt for Dashscope {
    async fn prompt(&self, prompt: Prompt) -> Result<String, LanguageModelError> {
        let model = self
            .default_options
            .prompt_model
            .as_ref()
            .context("Model not set")?
            .to_string();

        let request = CreateChatCompletionRequestArgs::default()
            .model(model)
            .messages(vec![ChatCompletionRequestUserMessageArgs::default()
                .content(prompt.render()?)
                .build()
                .map_err(openai_error_to_language_model_error)?
                .into()])
            .build()
            .map_err(openai_error_to_language_model_error)?;

        tracing::debug!(
            messages =
                serde_json::to_string_pretty(&request).map_err(LanguageModelError::permanent)?,
            "[SimplePrompt] Request to qwen"
        );

        let mut response = self
            .client
            .chat()
            .create(request)
            .await
            .map_err(openai_error_to_language_model_error)?;

        tracing::debug!(
            response =
                serde_json::to_string_pretty(&response).map_err(LanguageModelError::permanent)?,
            "[SimplePrompt] Response from qwen"
        );

        response
            .choices
            .remove(0)
            .message
            .content
            .take()
            .ok_or(LanguageModelError::PermanentError(
                "Expected content in response".into(),
            ))
    }
}
