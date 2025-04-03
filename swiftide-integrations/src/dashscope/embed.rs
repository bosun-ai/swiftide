use super::Dashscope;
use anyhow::{Context as _, Result};
use async_openai::types::CreateEmbeddingRequestArgs;
use async_trait::async_trait;
use swiftide_core::{chat_completion::errors::LanguageModelError, EmbeddingModel, Embeddings};

use crate::openai::openai_error_to_language_model_error;

#[async_trait]
impl EmbeddingModel for Dashscope {
    async fn embed(&self, input: Vec<String>) -> Result<Embeddings, LanguageModelError> {
        let model = self
            .default_options
            .embed_model
            .as_ref()
            .context("Model not set")
            .map_err(LanguageModelError::permanent)?;

        let dimensions = self.default_options.dimensions;
        let request = CreateEmbeddingRequestArgs::default()
            .model(model)
            .dimensions(dimensions)
            .input(&input)
            .build()
            .map_err(LanguageModelError::permanent)?;
        tracing::debug!(
            num_chunks = input.len(),
            model = &model,
            "[Embed] Request to qwen"
        );
        let response = self
            .client
            .embeddings()
            .create(request)
            .await
            .map_err(openai_error_to_language_model_error)?;

        let num_embeddings = response.data.len();
        tracing::debug!(num_embeddings = num_embeddings, "[Embed] Response openai");

        // WARN: Naively assumes that the order is preserved. Might not always be the case.
        Ok(response.data.into_iter().map(|d| d.embedding).collect())
    }
}
