use crate::{integrations::aws_bedrock::BedrockRequest, SimplePrompt};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use aws_sdk_bedrockruntime::primitives::Blob;

use super::{AwsBedrock, BedrockResponse, BedrockTextResult};

#[async_trait]
impl SimplePrompt for AwsBedrock {
    #[tracing::instrument(skip_all, err)]
    async fn prompt(&self, prompt: &str) -> Result<String> {
        let request = BedrockRequest::new(prompt.to_string(), self.model_config.clone());
        let Ok(prompt) = serde_json::to_vec(&request) else {
            anyhow::bail!("Failed to serialize prompt");
        };

        tracing::debug!(
            request = std::str::from_utf8(&prompt).unwrap(),
            "Sending request to bedrock model"
        );
        let blob = Blob::new(prompt);

        let response = self
            .client
            .invoke_model()
            .body(blob)
            .model_id(&self.model_id)
            .send()
            .await
            .map_err(|e| e.into_service_error())?;

        let response: &[u8] = &response.body.into_inner();
        let response = serde_json::from_slice::<BedrockResponse>(response)?;

        let Some(BedrockTextResult { output_text, .. }) = response.results.first() else {
            anyhow::bail!("Failed to get response");
        };

        Ok(output_text.clone())
    }
}
