use swiftide::{ingestion::IngestionPipeline, loaders::FileLoader, *};
use temp_dir::TempDir;
use testcontainers::runners::AsyncRunner;
use wiremock::MockServer;

// Test an ingestion pipeline without any mocks
#[test_log::test(tokio::test)]
async fn test_ingestion_pipeline() {
    let tempdir = TempDir::new().unwrap();
    let codefile = tempdir.child("main.rs");
    std::fs::write(&codefile, "fn main() { println!(\"Hello, World!\"); }").unwrap();

    let mock_server = MockServer::start().await;

    let config = async_openai::config::OpenAIConfig::new().with_api_base(mock_server.uri());
    let async_openai = async_openai::Client::with_config(config);

    let openai_client = integrations::openai::OpenAI::builder()
        .client(async_openai)
        .default_options(
            integrations::openai::Options::builder()
                .embed_model("text-embedding-3-small")
                .prompt_model("gpt-4o")
                .build()
                .unwrap(),
        )
        .build()
        .unwrap();

    // let redis = testcontainers::GenericImage::new("redis", "7.2.4")
    //     .with_exposed_port(6379)
    //     .with_wait_for(testcontainers::core::WaitFor::message_on_stdout(
    //         "Ready to accept connections",
    //     ))
    //     .start()
    //     .await
    //     .expect("Redis started");
    // let redis_url = format!(
    //     "redis://{host}:{port}",
    //     host = redis.get_host().await.unwrap(),
    //     port = redis.get_host_port_ipv4(6379).await.unwrap()
    // );

    let qdrant = testcontainers::GenericImage::new("qdrant/qdrant", "v1.9.2")
        .with_exposed_port(6334)
        .with_exposed_port(6333)
        .with_wait_for(testcontainers::core::WaitFor::message_on_stdout(
            "starting in Actix runtime",
        ))
        .start()
        .await
        .expect("Qdrant started");
    let qdrant_url = format!(
        "http://{host}:{port}",
        host = qdrant.get_host().await.unwrap(),
        port = qdrant.get_host_port_ipv4(6334).await.unwrap()
    );

    dbg!(&qdrant_url);
    // dbg!(qdrant.stdout_to_vec().await.map(String::from_utf8).unwrap());
    // dbg!(qdrant.stderr_to_vec().await.map(String::from_utf8).unwrap());

    IngestionPipeline::from_loader(FileLoader::new(tempdir.path()).with_extensions(&["rs"]))
        .then_chunk(transformers::ChunkCode::try_for_language("rust").unwrap())
        .then(transformers::MetadataQACode::new(openai_client.clone()))
        // .filter_cached(
        //     integrations::redis::RedisNodeCache::try_from_url(&redis_url, "prefix").unwrap(),
        // )
        .then_in_batch(1, transformers::OpenAIEmbed::new(openai_client.clone()))
        .store_with(
            integrations::qdrant::Qdrant::try_from_url(&qdrant_url)
                .unwrap()
                .vector_size(1536)
                .build()
                .unwrap(),
        )
        .run()
        .await
        .unwrap();
}
