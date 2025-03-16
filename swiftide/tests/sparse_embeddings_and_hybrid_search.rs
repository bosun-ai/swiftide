//! This module contains tests for the indexing pipeline in the Swiftide project.
//! The tests validate the functionality of the pipeline, ensuring it processes data correctly
//! from a temporary file, simulates API responses, and stores data accurately in the Qdrant vector
//! database.

use qdrant_client::qdrant::{
    Fusion, PrefetchQueryBuilder, Query, QueryPointsBuilder, ScrollPointsBuilder,
    SearchPointsBuilder, VectorInput,
};
use swiftide::indexing::*;
use swiftide::integrations;
use swiftide_integrations::fastembed::FastEmbed;
use swiftide_test_utils::*;
use temp_dir::TempDir;
use wiremock::MockServer;

/// Tests the indexing pipeline without any mocks.
///
/// This test sets up a temporary directory and file, simulates API responses using mock servers,
/// configures an OpenAI client, and runs the indexing pipeline. It then validates that the data
/// is correctly stored in the Qdrant vector database.
///
/// # Panics
/// Panics if any of the setup steps fail, such as creating the temporary directory or file,
/// starting the mock server, or configuring the OpenAI client.
///
/// # Errors
/// If the indexing pipeline encounters an error, the test will print the received requests
/// for debugging purposes.
#[test_log::test(tokio::test)]
async fn test_sparse_indexing_pipeline() {
    // Setup temporary directory and file for testing
    let tempdir = TempDir::new().unwrap();
    let codefile = tempdir.child("main.rs");
    std::fs::write(&codefile, "fn main() { println!(\"Hello, World!\"); }").unwrap();

    // Setup mock servers to simulate API responses
    let mock_server = MockServer::start().await;

    mock_embeddings(&mock_server, 1).await;

    let (qdrant_container, qdrant_url) = start_qdrant().await;
    let fastembed_sparse = FastEmbed::try_default_sparse().unwrap();
    let fastembed = FastEmbed::try_default().unwrap();
    let memory_storage = persist::MemoryStorage::default();

    println!("Qdrant URL: {qdrant_url}");

    let result =
        Pipeline::from_loader(loaders::FileLoader::new(tempdir.path()).with_extensions(&["rs"]))
            .then_chunk(transformers::ChunkCode::try_for_language("rust").unwrap())
            .then_in_batch(transformers::SparseEmbed::new(fastembed_sparse).with_batch_size(20))
            .then_in_batch(transformers::Embed::new(fastembed).with_batch_size(20))
            .log_nodes()
            .then_store_with(
                integrations::qdrant::Qdrant::try_from_url(&qdrant_url)
                    .unwrap()
                    .vector_size(384)
                    .with_vector(EmbeddedField::Combined)
                    .with_sparse_vector(EmbeddedField::Combined)
                    .collection_name("swiftide-test".to_string())
                    .build()
                    .unwrap(),
            )
            .then_store_with(memory_storage.clone())
            .run()
            .await;

    let node = memory_storage
        .get_all_values()
        .await
        .first()
        .unwrap()
        .clone();

    result.expect("Indexing pipeline failed");

    let qdrant_client = qdrant_client::Qdrant::from_url(&qdrant_url)
        .build()
        .unwrap();

    let stored_node = qdrant_client
        .scroll(
            ScrollPointsBuilder::new("swiftide-test")
                .limit(1)
                .with_payload(true)
                .with_vectors(true),
        )
        .await
        .unwrap();

    dbg!(stored_node);
    dbg!(
        std::str::from_utf8(&qdrant_container.stdout_to_vec().await.unwrap())
            .unwrap()
            .split('\n')
            .collect::<Vec<_>>()
    );

    // Search using the dense vector
    let dense = node
        .vectors
        .unwrap()
        .into_values()
        .collect::<Vec<_>>()
        .first()
        .cloned()
        .unwrap();
    let search_request = SearchPointsBuilder::new("swiftide-test", dense.as_slice(), 10)
        .with_payload(true)
        .vector_name(EmbeddedField::Combined);

    let search_response = qdrant_client.search_points(search_request).await.unwrap();
    let first = search_response.result.first().unwrap();

    assert!(first
        .payload
        .get("path")
        .unwrap()
        .as_str()
        .unwrap()
        .ends_with("main.rs"));
    assert_eq!(
        first.payload.get("content").unwrap().as_str().unwrap(),
        "fn main() { println!(\"Hello, World!\"); }"
    );

    // Search using the sparse vector
    let sparse = node
        .sparse_vectors
        .unwrap()
        .into_values()
        .collect::<Vec<_>>()
        .first()
        .cloned()
        .unwrap();

    // Search sparse
    let search_request = SearchPointsBuilder::new("swiftide-test", sparse.values.as_slice(), 10)
        .sparse_indices(sparse.indices.clone())
        .vector_name(format!("{}_sparse", EmbeddedField::Combined))
        .with_payload(true);

    let search_response = qdrant_client.search_points(search_request).await.unwrap();
    let first = search_response.result.first().unwrap();

    assert!(first
        .payload
        .get("path")
        .unwrap()
        .as_str()
        .unwrap()
        .ends_with("main.rs"));
    assert_eq!(
        first.payload.get("content").unwrap().as_str().unwrap(),
        "fn main() { println!(\"Hello, World!\"); }"
    );

    // Search hybrid
    let search_response = qdrant_client
        .query(
            QueryPointsBuilder::new("swiftide-test")
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

    let first = search_response.result.first().unwrap();

    assert!(first
        .payload
        .get("path")
        .unwrap()
        .as_str()
        .unwrap()
        .ends_with("main.rs"));
    assert_eq!(
        first.payload.get("content").unwrap().as_str().unwrap(),
        "fn main() { println!(\"Hello, World!\"); }"
    );
}
