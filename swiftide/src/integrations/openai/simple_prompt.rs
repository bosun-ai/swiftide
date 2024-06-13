use crate::SimplePrompt;
use async_openai::types::{ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs};
use async_trait::async_trait;

use super::OpenAI;
use anyhow::{Context as _, Result};

#[async_trait]
impl SimplePrompt for OpenAI {
    #[tracing::instrument(skip(self), err)]
    async fn prompt(&self, prompt: &str) -> Result<String> {
        let model = self
            .default_options
            .prompt_model
            .as_ref()
            .context("Model not set")?;

        let request = CreateChatCompletionRequestArgs::default()
            .model(model)
            .messages(vec![ChatCompletionRequestUserMessageArgs::default()
                .content(prompt)
                .build()?
                .into()])
            .build()?;

        tracing::debug!(
            messages = serde_json::to_string_pretty(&request)?,
            "[SimplePrompt] Request to openai"
        );

        let mut response = self.client.chat().create(request).await?;

        tracing::debug!(
            response = serde_json::to_string_pretty(&response)?,
            "[SimplePrompt] Response from openai"
        );

        response
            .choices
            .remove(0)
            .message
            .content
            .take()
            .context("Expected content in response")
    }
}
