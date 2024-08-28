use arrow_array::*;
use arrow_array::{
    cast::AsArray, Array, RecordBatch, StringArray,
};
use lancedb::query::ExecutableQuery;
use swiftide::{
    indexing::{
        transformers::{
            metadata_qa_code::NAME as METADATA_QA_CODE_NAME, ChunkCode, MetadataQACode,
        },
        EmbeddedField,
    },
    query::TryStreamExt as _,
};
use swiftide_indexing::{loaders, transformers, Pipeline};
use swiftide_integrations::{fastembed::FastEmbed, lancedb::LanceDB};
use swiftide_test_utils::{mock_chat_completions, openai_client};
use temp_dir::TempDir;
use wiremock::MockServer;

#[test_log::test(tokio::test)]
async fn test_lancedb() {
    // Setup temporary directory and file for testing
    let tempdir = TempDir::new().unwrap();
    let codefile = tempdir.child("main.rs");
    let code = "fn main() { println!(\"Hello, World!\"); }";
    std::fs::write(&codefile, code).unwrap();

    // Setup mock servers to simulate API responses
    let mock_server = MockServer::start().await;
    mock_chat_completions(&mock_server).await;

    let openai_client = openai_client(&mock_server.uri(), "text-embedding-3-small", "gpt-4o");

    let fastembed = FastEmbed::try_default().unwrap();

    let lancedb = LanceDB::builder()
        .uri(tempdir.child("lancedb").to_str().unwrap())
        .vector_size(384)
        .with_vector(EmbeddedField::Combined)
        .with_metadata(METADATA_QA_CODE_NAME)
        .table_name("swiftide_test")
        .build()
        .unwrap();

    Pipeline::from_loader(loaders::FileLoader::new(tempdir.path()).with_extensions(&["rs"]))
        .then_chunk(ChunkCode::try_for_language("rust").unwrap())
        .then(MetadataQACode::new(openai_client))
        .then_in_batch(20, transformers::Embed::new(fastembed))
        .log_nodes()
        .then_store_with(lancedb.clone())
        .run()
        .await
        .unwrap();

    // Assert that
    // * Vector got persisted
    // * Metadata field got persisted
    // * Indirectly correct table got created

    // Temporary tests to check before query pipeline
    let conn = lancedb.get_connection().await.unwrap();
    let table = conn.open_table("swiftide_test").execute().await.unwrap();

    let result: RecordBatch = table
        .query()
        .execute()
        .await
        .unwrap()
        .try_collect::<Vec<_>>()
        .await
        .unwrap()
        .first()
        .unwrap()
        .clone();

    assert_eq!(result.num_rows(), 1);
    assert_eq!(result.num_columns(), 4);
    assert!(result.column_by_name("id").is_some());
    assert_eq!(
        result
            .column_by_name("chunk")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>() // as_string() doesn't work, wtf
            .unwrap()
            .value(0),
        code
    );
    // assert_eq!(
    //     result
    //         .column_by_name("questions_and_answers__code_")
    //         .unwrap()
    //         .as_string_view()
    //         .value(0),
    //     "\n\nHello there, how may I assist you today?"
    // );
    assert_eq!(
        result
            .column_by_name("vector_combined")
            .unwrap()
            .as_fixed_size_list()
            .value(0)
            .len(),
        384
    );
}
