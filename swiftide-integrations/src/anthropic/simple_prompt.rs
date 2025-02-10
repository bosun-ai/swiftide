use anyhow::Context as _;
use async_anthropic::types::CreateMessagesRequestBuilder;
use async_trait::async_trait;
use swiftide_core::SimplePrompt;

use super::Anthropic;

#[async_trait]
impl SimplePrompt for Anthropic {
    #[tracing::instrument(skip_all, err)]
    async fn prompt(&self, prompt: swiftide_core::prompt::Prompt) -> anyhow::Result<String> {
        let model = self
            .default_options
            .prompt_model
            .as_ref()
            .context("Model not set")?;

        let request = CreateMessagesRequestBuilder::default()
            .model(model)
            .messages(vec![prompt.render().await?.into()])
            .build()?;

        tracing::debug!(
            model = &model,
            messages = serde_json::to_string_pretty(&request)?,
            "[SimplePrompt] Request to anthropic"
        );

        let response = self.client.messages().create(request).await?;

        tracing::debug!(
            response = serde_json::to_string_pretty(&response)?,
            "[SimplePrompt] Response from anthropic"
        );

        let message = response
            .messages()
            .into_iter()
            .next()
            .context("No messages in response")?;

        message.text().context("No text in response")
    }
}
