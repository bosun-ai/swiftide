use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::{EmbeddingModel, Embeddings};

use super::{EmbeddingModelType, FastEmbed};
#[async_trait]
impl EmbeddingModel for FastEmbed {
    #[tracing::instrument(skip_all)]
    async fn embed(&self, input: Vec<String>) -> Result<Embeddings> {
        if let EmbeddingModelType::Dense(embedding_model) = &*self.embedding_model {
            embedding_model.embed(input, self.batch_size)
        } else {
            Err(anyhow::anyhow!("Expected dense model, got sparse"))
        }
    }
}
