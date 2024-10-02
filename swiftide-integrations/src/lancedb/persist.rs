use std::sync::Arc;

use anyhow::Context as _;
use anyhow::Result;
use arrow_array::types::Float32Type;
use arrow_array::types::UInt8Type;
use arrow_array::types::Utf8Type;
use arrow_array::Array;
use arrow_array::FixedSizeListArray;
use arrow_array::GenericByteArray;
use arrow_array::RecordBatch;
use arrow_array::RecordBatchIterator;
use async_trait::async_trait;
use swiftide_core::indexing::IndexingStream;
use swiftide_core::indexing::Node;
use swiftide_core::Persist;

use super::FieldConfig;
use super::LanceDB;

#[async_trait]
impl Persist for LanceDB {
    #[tracing::instrument(skip_all)]
    async fn setup(&self) -> Result<()> {
        let conn = self.get_connection().await?;
        let schema = self.schema.clone();

        if let Err(err) = conn.open_table(&self.table_name).execute().await {
            if matches!(err, lancedb::Error::TableNotFound { .. }) {
                conn.create_empty_table(&self.table_name, schema)
                    .execute()
                    .await
                    .map(|_| ())
                    .map_err(anyhow::Error::from)?;
            } else {
                return Err(err.into());
            }
        }

        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn store(&self, node: Node) -> Result<Node> {
        let mut nodes = vec![node; 1];
        self.store_nodes(&nodes).await?;

        let node = nodes.swap_remove(0);

        Ok(node)
    }

    #[tracing::instrument(skip_all)]
    async fn batch_store(&self, nodes: Vec<Node>) -> IndexingStream {
        self.store_nodes(&nodes).await.map(|()| nodes).into()
    }

    fn batch_size(&self) -> Option<usize> {
        Some(self.batch_size)
    }
}

impl LanceDB {
    async fn store_nodes(&self, nodes: &[Node]) -> Result<()> {
        let schema = self.schema.clone();

        let batches = self.extract_arrow_batches_from_nodes(nodes)?;

        let data = RecordBatchIterator::new(
            vec![RecordBatch::try_new(schema.clone(), batches)
                .context("Could not create batches")?]
            .into_iter()
            .map(Ok),
            schema.clone(),
        );

        let conn = self.get_connection().await?;
        let table = conn.open_table(&self.table_name).execute().await?;
        let mut merge_insert = table.merge_insert(&["id"]);

        merge_insert
            .when_matched_update_all(None)
            .when_not_matched_insert_all();

        merge_insert.execute(Box::new(data)).await?;

        Ok(())
    }

    fn extract_arrow_batches_from_nodes(
        &self,
        nodes: &[Node],
    ) -> core::result::Result<Vec<Arc<dyn Array>>, anyhow::Error> {
        let fields = self.fields.as_slice();
        let mut batches: Vec<Arc<dyn Array>> = Vec::with_capacity(fields.len());

        for field in fields {
            match field {
                FieldConfig::Vector(config) => {
                    let mut row = Vec::with_capacity(nodes.len());
                    let vector_size = config
                        .vector_size
                        .or(self.vector_size)
                        .context("Expected vector size to be set for field")?;

                    for node in nodes {
                        let data = node
                            .vectors
                            .as_ref()
                            // TODO: verify compiler optimizes the double loops away
                            .and_then(|v| v.get(&config.embedded_field))
                            .map(|v| v.iter().map(|f| Some(*f)));

                        row.push(data);
                    }
                    batches.push(Arc::new(FixedSizeListArray::from_iter_primitive::<
                        Float32Type,
                        _,
                        _,
                    >(row, vector_size)));
                }
                FieldConfig::Metadata(config) => {
                    let mut row = Vec::with_capacity(nodes.len());

                    for node in nodes {
                        let data = node
                            .metadata
                            .get(&config.original_field)
                            // TODO: Verify this gives the correct data
                            .and_then(|v| v.as_str());

                        row.push(data);
                    }
                    batches.push(Arc::new(GenericByteArray::<Utf8Type>::from_iter(row)));
                }
                FieldConfig::Chunk => {
                    let mut row = Vec::with_capacity(nodes.len());

                    for node in nodes {
                        let data = Some(node.chunk.as_str());
                        row.push(data);
                    }
                    batches.push(Arc::new(GenericByteArray::<Utf8Type>::from_iter(row)));
                }
                FieldConfig::ID => {
                    let mut row = Vec::with_capacity(nodes.len());
                    for node in nodes {
                        let data = Some(node.id().as_bytes().map(Some));
                        row.push(data);
                    }
                    batches.push(Arc::new(FixedSizeListArray::from_iter_primitive::<
                        UInt8Type,
                        _,
                        _,
                    >(row, 16)));
                }
            }
        }
        Ok(batches)
    }
}

#[cfg(test)]
mod test {
    use swiftide_core::{indexing::EmbeddedField, Persist as _};
    use temp_dir::TempDir;

    use super::*;

    async fn setup() -> (TempDir, LanceDB) {
        let tempdir = TempDir::new().unwrap();
        let lancedb = LanceDB::builder()
            .uri(tempdir.child("lancedb").to_str().unwrap())
            .vector_size(384)
            .with_metadata("filter")
            .with_vector(EmbeddedField::Combined)
            .table_name("swiftide_test")
            .build()
            .unwrap();
        lancedb.setup().await.unwrap();

        (tempdir, lancedb)
    }

    #[tokio::test]
    async fn test_no_error_when_table_exists() {
        let (_guard, lancedb) = setup().await;

        lancedb
            .setup()
            .await
            .expect("Should not error if table exists");
    }
}
