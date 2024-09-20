use std::sync::Arc;

use swiftide_core::{
    indexing::EmbeddingModel,
    prelude::*,
    querying::{states, Query, TransformQuery},
};

#[derive(Debug, Clone)]
pub struct Embed {
    embed_model: Arc<dyn EmbeddingModel>,
}

impl Embed {
    pub fn from_client(client: impl EmbeddingModel + 'static) -> Embed {
        Embed {
            embed_model: Arc::new(client),
        }
    }
}

#[async_trait]
impl TransformQuery for Embed {
    #[tracing::instrument(skip_all)]
    async fn transform_query(
        &self,
        mut query: Query<states::Pending>,
    ) -> Result<Query<states::Pending>> {
        let Some(embedding) = self
            .embed_model
            .embed(vec![query.current().to_string()])
            .await?
            .pop()
        else {
            anyhow::bail!("Failed to embed query")
        };

        query.embedding = Some(embedding);

        Ok(query)
    }
}
