use crate::SimplePrompt;
use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_bedrockruntime::primitives::Blob;

use super::AwsBedrock;

#[async_trait]
impl SimplePrompt for AwsBedrock {
    #[tracing::instrument(skip_all, err)]
    async fn prompt(&self, prompt: &str) -> Result<String> {
        let blob = self
            .model_family
            .build_request_to_bytes(prompt, &self.model_config)
            .map(Blob::new)?;

        let response = self
            .client
            .invoke_model()
            .body(blob)
            .model_id(&self.model_id)
            .send()
            .await
            .map_err(|e| e.into_service_error())?;

        let response_bytes: &[u8] = &response.body.into_inner();

        tracing::debug!(
            "Received response: {:?}",
            std::str::from_utf8(response_bytes)?
        );

        self.model_family.output_message_from_bytes(response_bytes)
    }
}
