//! # [Swiftide] Hybrid search with qudrant
//!
//! This example demonstrates how to do hybrid search with Qdrant with Sparse vectors.
//!
//! [Swiftide]: https://github.com/bosun-ai/swiftide
//! [examples]: https://github.com/bosun-ai/swiftide/blob/master/examples

use qdrant_client::qdrant::{Fusion, PrefetchQueryBuilder, Query, QueryPointsBuilder, VectorInput};
use swiftide::{
    indexing::{
        self,
        loaders::FileLoader,
        transformers::{self, ChunkCode},
        EmbeddedField, EmbeddingModel as _, SparseEmbeddingModel as _,
    },
    integrations::{fastembed::FastEmbed, qdrant::Qdrant},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    // Ensure all batching is consistent
    let batch_size = 256;

    let fastembed_sparse = FastEmbed::try_default_sparse()
        .unwrap()
        .with_batch_size(batch_size)
        .to_owned();
    let fastembed = FastEmbed::try_default()
        .unwrap()
        .with_batch_size(batch_size)
        .to_owned();

    indexing::Pipeline::from_loader(FileLoader::new("swiftide-core/").with_extensions(&["rs"]))
        .then_chunk(ChunkCode::try_for_language_and_chunk_size(
            "rust",
            10..2048,
        )?)
        .then_in_batch(
            batch_size,
            transformers::SparseEmbed::new(fastembed_sparse.clone()),
        )
        .then_in_batch(batch_size, transformers::Embed::new(fastembed.clone()))
        .then_store_with(
            Qdrant::builder()
                .batch_size(batch_size)
                .vector_size(384)
                .with_vector(EmbeddedField::Combined)
                .with_sparse_vector(EmbeddedField::Combined)
                .collection_name("swiftide-hybrid")
                .build()?,
        )
        .run()
        .await?;

    let query = "Where is the indexing pipeline defined?";

    let sparse = fastembed_sparse
        .sparse_embed(vec![query.to_string()])
        .await?
        .first()
        .unwrap()
        .to_owned();

    let dense = fastembed
        .embed(vec![query.to_string()])
        .await?
        .first()
        .unwrap()
        .to_owned();

    let qdrant = qdrant_client::Qdrant::from_url("http://localhost:6334")
        .build()
        .unwrap();

    let search_response = qdrant
        .query(
            QueryPointsBuilder::new("swiftide-hybrid")
                .with_payload(true)
                .add_prefetch(
                    PrefetchQueryBuilder::default()
                        .query(Query::new_nearest(VectorInput::new_sparse(
                            sparse.indices,
                            sparse.values,
                        )))
                        .using("Combined_sparse")
                        .limit(20u64),
                )
                .add_prefetch(
                    PrefetchQueryBuilder::default()
                        .query(Query::new_nearest(dense))
                        .using("Combined")
                        .limit(20u64),
                )
                .query(Query::new_fusion(Fusion::Rrf)),
        )
        .await
        .unwrap();

    for result in search_response.result {
        println!("---");
        println!("{:?}", result);
        println!("---");
    }

    Ok(())
}
