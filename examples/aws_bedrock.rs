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
    SimplePrompt as _,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let aws_bedrock = integrations::aws_bedrock::AwsBedrock::builder()
        .model_id("amazon.titan-text-express-v1")
        .build()?;

    let memory_storage = MemoryStorage::default();

    ingestion::IngestionPipeline::from_loader(FileLoader::new(".").with_extensions(&["md"]))
        .log_nodes()
        .then(transformers::MetadataSummary::new(aws_bedrock.clone()))
        .then_store_with(memory_storage.clone())
        .log_all()
        .run()
        .await?;

    println!("Summaries:");
    println!(
        "{}",
        memory_storage
            .get_all()
            .await
            .iter()
            .filter_map(|n| n.metadata.get("Summary"))
            .cloned()
            .collect::<Vec<_>>()
            .join("\n---\n")
    );
    Ok(())
}
