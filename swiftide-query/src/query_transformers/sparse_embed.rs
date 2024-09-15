use std::sync::Arc;

use swiftide_core::{
    prelude::*,
    querying::{states, Query, TransformQuery},
    SparseEmbeddingModel,
};

/// Embed a query with a sparse embedding.
#[derive(Debug, Clone)]
pub struct SparseEmbed {
    embed_model: Arc<dyn SparseEmbeddingModel>,
}

impl SparseEmbed {
    pub fn from_client(client: impl SparseEmbeddingModel + 'static) -> SparseEmbed {
        SparseEmbed {
            embed_model: Arc::new(client),
        }
    }
}

#[async_trait]
impl TransformQuery for SparseEmbed {
    #[tracing::instrument(skip_all)]
    async fn transform_query(
        &self,
        mut query: Query<states::Pending>,
    ) -> Result<Query<states::Pending>> {
        let Some(embedding) = self
            .embed_model
            .sparse_embed(vec![query.current().to_string()])
            .await?
            .pop()
        else {
            anyhow::bail!("Failed to embed query")
        };

        query.sparse_embedding = Some(embedding);

        Ok(query)
    }
}
