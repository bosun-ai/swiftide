//! # [Swiftide] Aws Bedrock example
//!
//! This example demonstrates how to ingest the Swiftide codebase itself using FastEmbed.
//!
//! The pipeline will:
//! - Load all `.rs` files from the current directory
//! - Embed the chunks in batches of 10 using FastEmbed
//! - Store the nodes in Qdrant
//!
//! [Swiftide]: https://github.com/bosun-ai/swiftide
//! [examples]: https://github.com/bosun-ai/swiftide/blob/master/examples

use swiftide::{
    ingestion,
    integrations::{self, fastembed::FastEmbed},
    loaders::FileLoader,
    persist::MemoryStorage,
    transformers::{self, Embed},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let aws_bedrock = integrations::aws_bedrock::AwsBedrock::builder()
        .model_id("model_id".to_string())
        .build()?;

    ingestion::IngestionPipeline::from_loader(FileLoader::new(".").with_extensions(&["md"]))
        .then(transformers::MetadataSummary::new(aws_bedrock.clone()))
        .then_in_batch(10, Embed::new(FastEmbed::builder().batch_size(10).build()?))
        .then_store_with(MemoryStorage::default())
        .log_all()
        .run()
        .await?;

    println!("Ingestion done");
    println!("{:?}", MemoryStorage::default().get_all().await);
    Ok(())
}
