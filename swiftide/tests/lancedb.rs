use swiftide::indexing::{
    transformers::{ChunkCode, MetadataQACode},
    EmbeddedField,
};
use swiftide_indexing::{loaders, persist, transformers, Pipeline};
use swiftide_integrations::{fastembed::FastEmbed, lancedb::LanceDB};
use swiftide_test_utils::{mock_chat_completions, mock_embeddings, openai_client};
use temp_dir::TempDir;
use wiremock::MockServer;

#[test_log::test(tokio::test)]
async fn test_sparse_indexing_pipeline() {
    // Setup temporary directory and file for testing
    let tempdir = TempDir::new().unwrap();
    let codefile = tempdir.child("main.rs");
    std::fs::write(&codefile, "fn main() { println!(\"Hello, World!\"); }").unwrap();

    // Setup mock servers to simulate API responses
    let mock_server = MockServer::start().await;
    mock_chat_completions(&mock_server).await;

    let openai_client = openai_client(&mock_server.uri(), "text-embedding-3-small", "gpt-4o");

    let fastembed_sparse = FastEmbed::try_default_sparse().unwrap();
    let fastembed = FastEmbed::try_default().unwrap();
    let memory_storage = persist::MemoryStorage::default();

    let result =
        Pipeline::from_loader(loaders::FileLoader::new(tempdir.path()).with_extensions(&["rs"]))
            .then_chunk(ChunkCode::try_for_language("rust").unwrap())
            .then(MetadataQACode::new(openai_client))
            .then_in_batch(20, transformers::SparseEmbed::new(fastembed_sparse))
            .then_in_batch(20, transformers::Embed::new(fastembed))
            .log_nodes()
            .then_store_with(
                LanceDB::builder()
                    .uri(tempdir.child("lancedb").to_str().unwrap())
                    .vector_size(384)
                    .with_vector(EmbeddedField::Combined)
                    .with_sparse_vector(EmbeddedField::Combined)
                    .table_name("swiftide_test")
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
}
