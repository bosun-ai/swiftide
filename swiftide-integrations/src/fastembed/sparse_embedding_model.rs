use async_trait::async_trait;
use swiftide_core::chat_completion::errors::LanguageModelError;
use swiftide_core::{SparseEmbedding, SparseEmbeddingModel, SparseEmbeddings};

use super::{EmbeddingModelType, FastEmbed};
#[async_trait]
impl SparseEmbeddingModel for FastEmbed {
    #[tracing::instrument(skip_all)]
    async fn sparse_embed(
        &self,
        input: Vec<String>,
    ) -> Result<SparseEmbeddings, LanguageModelError> {
        let mut embedding_model = self.embedding_model.lock().await;

        match &mut *embedding_model {
            EmbeddingModelType::Sparse(model) => model
                .embed(input, self.batch_size)
                .map_err(LanguageModelError::permanent)
                .and_then(|embeddings| {
                    embeddings
                        .into_iter()
                        .map(|embedding| {
                            let indices = embedding
                                .indices
                                .iter()
                                .map(|v| u32::try_from(*v).map_err(LanguageModelError::permanent))
                                .collect::<Result<Vec<_>, LanguageModelError>>()?;

                            Ok(SparseEmbedding {
                                indices,
                                values: embedding.values,
                            })
                        })
                        .collect()
                }),
            EmbeddingModelType::Dense(_) => Err(LanguageModelError::PermanentError(
                "Expected sparse model, got dense".into(),
            )),
        }
    }
}
