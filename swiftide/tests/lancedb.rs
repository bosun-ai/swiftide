use swiftide::indexing;
use swiftide::query::{self, states, Query, TransformationEvent};
use swiftide::indexing::{
        transformers::{
            metadata_qa_code::NAME as METADATA_QA_CODE_NAME, ChunkCode, MetadataQACode,
        },
        EmbeddedField,
    };
use swiftide_indexing::{loaders, transformers, Pipeline};
use swiftide_integrations::{fastembed::FastEmbed, lancedb::LanceDB};
use swiftide_query::{answers, query_transformers, response_transformers};
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
        .with_metadata("filter")
        .table_name("swiftide_test")
        .build()
        .unwrap();

    Pipeline::from_loader(loaders::FileLoader::new(tempdir.path()).with_extensions(&["rs"]))
        .then_chunk(ChunkCode::try_for_language("rust").unwrap())
        .then(MetadataQACode::new(openai_client.clone()))
        .then(|mut node: indexing::Node| {
            node.metadata
                .insert("filter".to_string(), "true".to_string());
            Ok(node)
        })
        .then_in_batch(transformers::Embed::new(fastembed.clone()).with_batch_size(20))
        .log_nodes()
        .then_store_with(lancedb.clone())
        .run()
        .await
        .unwrap();

    let strategy = query::search_strategies::SimilaritySingleEmbedding::from_filter(
        "filter = \"true\"".to_string(),
    );

    let query_pipeline = query::Pipeline::from_search_strategy(strategy)
        .then_transform_query(query_transformers::GenerateSubquestions::from_client(
            openai_client.clone(),
        ))
        .then_transform_query(query_transformers::Embed::from_client(fastembed.clone()))
        .then_retrieve(lancedb.clone())
        .then_transform_response(response_transformers::Summary::from_client(
            openai_client.clone(),
        ))
        .then_answer(answers::Simple::from_client(openai_client.clone()));

    let result: Query<states::Answered> = query_pipeline.query("What is swiftide?").await.unwrap();

    dbg!(&result);

    assert_eq!(
        result.answer(),
        "\n\nHello there, how may I assist you today?"
    );
    let TransformationEvent::Retrieved { documents, .. } = result
        .history()
        .iter()
        .find(|e| matches!(e, TransformationEvent::Retrieved { .. }))
        .unwrap()
    else {
        panic!("No documents found")
    };

    assert_eq!(
        documents.first().unwrap(),
        "fn main() { println!(\"Hello, World!\"); }"
    );
}
