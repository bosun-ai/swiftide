use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::{EmbeddingModel, Embeddings, chat_completion::errors::LanguageModelError};

use super::{EmbeddingModelType, FastEmbed};
#[async_trait]
impl EmbeddingModel for FastEmbed {
    #[tracing::instrument(skip_all)]
    async fn embed(&self, input: Vec<String>) -> Result<Embeddings, LanguageModelError> {
        let mut embedding_model = self.embedding_model.lock().await;

        match &mut *embedding_model {
            EmbeddingModelType::Dense(model) => model
                .embed(input, self.batch_size)
                .map_err(LanguageModelError::permanent),
            EmbeddingModelType::Sparse(_) => Err(LanguageModelError::PermanentError(
                "Expected dense model, got sparse".into(),
            )),
        }
    }
}
