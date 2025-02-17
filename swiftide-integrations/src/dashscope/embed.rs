use super::Dashscope;
use anyhow::{Context as _, Result};
use async_openai::types::CreateEmbeddingRequestArgs;
use async_trait::async_trait;
use swiftide_core::{EmbeddingModel, Embeddings};

#[async_trait]
impl EmbeddingModel for Dashscope {
    async fn embed(&self, input: Vec<String>) -> Result<Embeddings> {
        let model = self
            .default_options
            .embed_model
            .as_ref()
            .context("Model not set")?;
        let dimensions = self.default_options.dimensions;
        let request = CreateEmbeddingRequestArgs::default()
            .model(model)
            .dimensions(dimensions)
            .input(&input)
            .build()?;
        tracing::debug!(
            num_chunks = input.len(),
            model = &model,
            "[Embed] Request to qwen"
        );
        let response = self.client.embeddings().create(request).await?;

        let num_embeddings = response.data.len();
        tracing::debug!(num_embeddings = num_embeddings, "[Embed] Response openai");

        // WARN: Naively assumes that the order is preserved. Might not always be the case.
        Ok(response.data.into_iter().map(|d| d.embedding).collect())
    }
}
