use anyhow::Context as _;
use arrow_array::StringArray;
use futures_util::{StreamExt as _, TryStreamExt as _};
use parquet::arrow::{ParquetRecordBatchStreamBuilder, ProjectionMask};
use swiftide_core::{
    indexing::{IndexingStream, Node},
    Loader,
};
use tokio::{fs::File, runtime::Handle};

use super::Parquet;

impl Loader for Parquet {
    fn into_stream(self) -> IndexingStream {
        let mut builder = tokio::task::block_in_place(|| {
            Handle::current().block_on(async {
                let file = File::open(self.path).await.expect("Failed to open file");

                ParquetRecordBatchStreamBuilder::new(file)
                    .await
                    .context("Failed to load builder")
                    .unwrap()
                    .with_batch_size(self.batch_size)
            })
        });

        let file_metadata = builder.metadata().file_metadata().clone();
        dbg!(file_metadata.schema_descr().columns());
        let column_idx = file_metadata
            .schema()
            .get_fields()
            .iter()
            .enumerate()
            .find_map(|(pos, column)| {
                if self.column_name == column.name() {
                    Some(pos)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| panic!("Column {} not found in dataset", &self.column_name));

        let mask = ProjectionMask::roots(file_metadata.schema_descr(), [column_idx]);
        builder = builder.with_projection(mask);

        let stream = builder.build().expect("Failed to build parquet builder");

        let swiftide_stream = stream
            .map_ok(move |batch| {
                assert!(batch.num_columns() == 1, "Number of columns _must_ be 1");

                let node_values = batch
                    .column(0) // Should only have one column at this point
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .unwrap()
                    .into_iter()
                    .flatten()
                    .map(Node::from)
                    .map(Ok)
                    .collect::<Vec<_>>();

                IndexingStream::iter(node_values)
            })
            .map_err(|e| anyhow::anyhow!("Error loading parquet batch {e}"));

        swiftide_stream.boxed().try_flatten().boxed().into()

        // let mask = ProjectionMask::
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test_log::test(tokio::test(flavor = "multi_thread"))]
    async fn test_parquet_loader() {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("src/parquet/test.parquet");
        dbg!(&path);

        let loader = Parquet::builder()
            .path(path)
            .column_name("chunk")
            .build()
            .unwrap();

        let result = loader.into_stream().try_collect::<Vec<_>>().await.unwrap();

        let expected = [Node::new("hello"), Node::new("world")];
        assert_eq!(result, expected);
    }
}
