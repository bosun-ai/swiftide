use async_openai::types::{ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs};
use async_trait::async_trait;
use swiftide_core::{prompt::Prompt, SimplePrompt};

use super::Dashscope;
use anyhow::{Context as _, Result};

#[async_trait]
impl SimplePrompt for Dashscope {
    async fn prompt(&self, prompt: Prompt) -> Result<String> {
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
                .build()?
                .into()])
            .build()?;

        tracing::debug!(
            messages = serde_json::to_string_pretty(&request)?,
            "[SimplePrompt] Request to qwen"
        );

        let mut response = self.client.chat().create(request).await?;

        tracing::debug!(
            response = serde_json::to_string_pretty(&response)?,
            "[SimplePrompt] Response from qwen"
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
