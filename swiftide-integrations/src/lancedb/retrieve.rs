use anyhow::{Context as _, Result};
use arrow_array::StringArray;
use async_trait::async_trait;
use futures_util::TryStreamExt;
use itertools::Itertools;
use lancedb::query::{ExecutableQuery, QueryBase as _};
use swiftide_core::{
    querying::{search_strategies::SimilaritySingleEmbedding, states, Query},
    Retrieve,
};

use super::{FieldConfig, LanceDB};

#[async_trait]
impl Retrieve<SimilaritySingleEmbedding> for LanceDB {
    #[tracing::instrument]
    async fn retrieve(
        &self,
        search_strategy: &SimilaritySingleEmbedding,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>> {
        let Some(embedding) = &query.embedding else {
            anyhow::bail!("No embedding for query")
        };

        let table = self
            .get_connection()
            .await?
            .open_table(&self.table_name)
            .execute()
            .await?;

        let vector_fields = self
            .fields
            .iter()
            .filter(|field| matches!(field, FieldConfig::Vector(_)))
            .collect_vec();

        if vector_fields.is_empty() || vector_fields.len() > 1 {
            anyhow::bail!("Zero or multiple vector fields configured in schema")
        }

        let column_name = vector_fields.first().map(|v| v.field_name()).unwrap();

        let result = table
            .query()
            .nearest_to(embedding.as_slice())?
            .column(&column_name)
            .limit(usize::try_from(search_strategy.top_k())?)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?
            .first()
            .context("Failed to retrieve documents")?
            .to_owned();

        let documents: Vec<String> = result
            .column_by_name("chunk")
            .and_then(|raw_array| raw_array.as_any().downcast_ref::<StringArray>())
            .context("Could not cast documents to strings")?
            .iter()
            .flatten()
            .map_into()
            .collect();

        Ok(query.retrieved_documents(documents))
    }
}
