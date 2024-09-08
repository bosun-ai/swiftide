use anyhow::{Context as _, Result};
use async_trait::async_trait;

use ollama_rs::generation::embeddings::request::GenerateEmbeddingsRequest;
use swiftide_core::{EmbeddingModel, Embeddings};

use super::Ollama;

#[async_trait]
impl EmbeddingModel for Ollama {
    async fn embed(&self, input: Vec<String>) -> Result<Embeddings> {
        let model = self
            .default_options
            .embed_model
            .as_ref()
            .context("Model not set")?;

        let request = GenerateEmbeddingsRequest::new(model.to_string(), input.into());
        tracing::debug!(
            messages = serde_json::to_string_pretty(&request)?,
            "[Embed] Request to ollama"
        );
        let response = self
            .client
            .generate_embeddings(request)
            .await
            .context("Request to Ollama Failed")?;

        tracing::debug!("[Embed] Response ollama");

        Ok(response.embeddings)
    }
}
