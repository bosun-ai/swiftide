use anyhow::Result;
use async_openai::types::CreateEmbeddingRequestArgs;
use async_trait::async_trait;

use crate::{Embed, Embeddings};

use super::OpenAI;

#[async_trait]
impl Embed for OpenAI {
    async fn embed(&self, input: Vec<String>) -> Result<Embeddings> {
        let request = CreateEmbeddingRequestArgs::default()
            .model(&self.embed_model)
            .input(input)
            .build()?;
        tracing::debug!(
            messages = serde_json::to_string_pretty(&request)?,
            "[Embed] Request to openai"
        );
        let response = self.client.embeddings().create(request).await?;
        tracing::debug!("[Embed] Response openai");

        // WARN: Naively assumes that the order is preserved. Might not always be the case.
        Ok(response.data.into_iter().map(|d| d.embedding).collect())
    }
}
