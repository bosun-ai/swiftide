use anyhow::Context;
use lancedb::query::{self as lance_query_builder, QueryBase};
use swiftide::indexing;
use swiftide::indexing::{
    transformers::{metadata_qa_code::NAME as METADATA_QA_CODE_NAME, ChunkCode, MetadataQACode},
    EmbeddedField,
};
use swiftide::query::{self as swift_query_pipeline, states, Query};
use swiftide_indexing::{loaders, transformers, Pipeline};
use swiftide_integrations::{
    fastembed::FastEmbed,
    lancedb::{self as lance_integration, LanceDB},
};
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
        .with_metadata("path")
        .table_name("swiftide_test")
        .build()
        .unwrap();

    Pipeline::from_loader(loaders::FileLoader::new(tempdir.path()).with_extensions(&["rs"]))
        .then_chunk(ChunkCode::try_for_language("rust").unwrap())
        .then(MetadataQACode::new(openai_client.clone()))
        .then(|mut node: indexing::Node| {
            // Add path to metadata, by default, storage will store all metadata fields
            node.metadata
                .insert("path", node.path.display().to_string());
            node.metadata.insert("filter", "true");
            Ok(node)
        })
        .then_in_batch(transformers::Embed::new(fastembed.clone()).with_batch_size(20))
        .log_nodes()
        .then_store_with(lancedb.clone())
        .run()
        .await
        .unwrap();

    let strategy = swift_query_pipeline::search_strategies::SimilaritySingleEmbedding::from_filter(
        "filter = \"true\"".to_string(),
    );

    let query_pipeline = swift_query_pipeline::Pipeline::from_search_strategy(strategy)
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

    let retrieved_document = result.documents().first().unwrap();
    assert_eq!(retrieved_document.content(), code);

    assert_eq!(
        retrieved_document.metadata().get("path").unwrap(),
        codefile.to_str().unwrap()
    );
}

#[test_log::test(tokio::test)]
async fn test_lancedb_retrieve_dynamic_search() {
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
        .with_metadata("path")
        .table_name("swiftide_test")
        .build()
        .unwrap();

    Pipeline::from_loader(loaders::FileLoader::new(tempdir.path()).with_extensions(&["rs"]))
        .then_chunk(ChunkCode::try_for_language("rust").unwrap())
        .then(MetadataQACode::new(openai_client.clone()))
        .then(|mut node: indexing::Node| {
            // Add path to metadata, by default, storage will store all metadata fields
            node.metadata
                .insert("path", node.path.display().to_string());
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

    // Create the custom query strategy for vector similarity search
    let create_vector_search_strategy =
        |lancedb: &LanceDB,
         table_name: String|
         -> swift_query_pipeline::search_strategies::CustomStrategy<
            lance_query_builder::VectorQuery,
        > {
            let table_name = table_name.clone();
            let lancedb = lancedb.clone();

            swift_query_pipeline::search_strategies::CustomStrategy::from_async_query(
                move |query_node| {
                    // Create owned copies for the async block
                    let table_name = table_name.clone();
                    let lancedb = lancedb.clone();

                    let embedding = if let Some(embedding) = &query_node.embedding {
                        embedding.clone()
                    } else {
                        panic!("Query embedding not found");
                    };

                    // Return a Future using async block syntax
                    Box::pin(async move {
                        // Create a new connection for each query execution
                        let connection = lancedb.get_connection().await?;

                        // Open the table within the query execution context
                        let vector_table = connection
                            .open_table(&table_name)
                            .execute()
                            .await
                            .context("Failed to open vector search table")?;

                        let vector_field =
                            lance_integration::VectorConfig::from(EmbeddedField::Combined)
                                .field_name();

                        // Build and return the query
                        let query_builder = vector_table
                            .query()
                            .nearest_to(embedding.as_slice())?
                            .column(&vector_field)
                            .limit(20);

                        Ok(query_builder)
                        // Connection is dropped here when query_builder is executed
                    })
                },
            )
        };

    let vector_search_strategy =
        create_vector_search_strategy(&lancedb, "swiftide_test".to_string());

    let query_pipeline =
        swift_query_pipeline::Pipeline::from_search_strategy(vector_search_strategy)
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

    let retrieved_document = result.documents().first().unwrap();
    assert_eq!(retrieved_document.content(), code);

    assert_eq!(
        retrieved_document.metadata().get("path").unwrap(),
        codefile.to_str().unwrap()
    );
}
