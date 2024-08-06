use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::{SparseEmbedding, SparseEmbeddingModel, SparseEmbeddings};

use super::{EmbeddingModelType, FastEmbed};
#[async_trait]
impl SparseEmbeddingModel for FastEmbed {
    #[tracing::instrument(skip_all)]
    async fn sparse_embed(&self, input: Vec<String>) -> Result<SparseEmbeddings> {
        if let EmbeddingModelType::Sparse(embedding_model) = &*self.embedding_model {
            embedding_model
                .embed(input, self.batch_size)
                .map(|embeddings| {
                    embeddings
                        .into_iter()
                        .map(|embedding| SparseEmbedding {
                            indices: embedding.indices,
                            values: embedding.values,
                        })
                        .collect()
                })
        } else {
            Err(anyhow::anyhow!("Expected dense model, got sparse"))
        }
    }
}
