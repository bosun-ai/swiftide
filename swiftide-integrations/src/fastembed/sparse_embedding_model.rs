use anyhow::{Context as _, Result};
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
                .and_then(|embeddings| {
                    embeddings
                        .into_iter()
                        .map(|embedding| {
                            Ok(SparseEmbedding {
                                indices: embedding
                                    .indices
                                    .iter()
                                    .map(|v| {
                                        u32::try_from(*v).context(
                                            "Could not convert sparse vector from u32 to usize",
                                        )
                                    })
                                    .collect::<Result<Vec<_>>>()?,
                                values: embedding.values,
                            })
                        })
                        .collect()
                })
        } else {
            Err(anyhow::anyhow!("Expected dense model, got sparse"))
        }
    }
}
