use anyhow::{Context as _, Result};
use arrow_array::StringArray;
use fs_err::tokio::File;
use futures_util::StreamExt as _;
use parquet::arrow::{ParquetRecordBatchStreamBuilder, ProjectionMask};
use swiftide_core::{
    indexing::{IndexingStream, Node},
    Loader,
};
use tokio::runtime::Handle;

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

        let swiftide_stream = stream.flat_map_unordered(None, move |result_batch| {
            let Ok(batch) = result_batch else {
                let new_result: Result<Node> = Err(anyhow::anyhow!(result_batch.unwrap_err()));

                return vec![new_result].into();
            };
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
        });

        swiftide_stream.boxed().into()

        // let mask = ProjectionMask::
    }

    fn into_stream_boxed(self: Box<Self>) -> IndexingStream {
        self.into_stream()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use futures_util::TryStreamExt as _;

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
