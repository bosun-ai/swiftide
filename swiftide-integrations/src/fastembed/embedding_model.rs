use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::{chat_completion::errors::LanguageModelError, EmbeddingModel, Embeddings};

use super::{EmbeddingModelType, FastEmbed};
#[async_trait]
impl EmbeddingModel for FastEmbed {
    #[tracing::instrument(skip_all)]
    async fn embed(&self, input: Vec<String>) -> Result<Embeddings, LanguageModelError> {
        if let EmbeddingModelType::Dense(embedding_model) = &*self.embedding_model {
            embedding_model
                .embed(input, self.batch_size)
                .map_err(LanguageModelError::permanent)
        } else {
            Err(LanguageModelError::PermanentError(
                "Expected dense model, got sparse".into(),
            ))
        }
    }
}
