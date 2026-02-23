//! # [Swiftide] Aws Bedrock example
//!
//! This example demonstrates how to use the `AwsBedrock` v2 integration to interact with Bedrock
//! service.
//!
//! To use bedrock you will need the following:
//! - AWS cli or environment variables configured
//! - An aws region configured
//! - Access to the bedrock models you want to use
//! - A model id or arn
//!
//! [Swiftide]: https://github.com/bosun-ai/swiftide
//! [examples]: https://github.com/bosun-ai/swiftide/blob/master/examples
//! [AWS Bedrock documentation]: https://docs.aws.amazon.com/bedrock/

use swiftide::{
    indexing, indexing::loaders::FileLoader, indexing::persist::MemoryStorage,
    indexing::transformers, integrations,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let aws_bedrock = integrations::aws_bedrock_v2::AwsBedrock::builder()
        .default_prompt_model("anthropic.claude-3-5-sonnet-20241022-v2:0")
        .build()?;

    let memory_storage = MemoryStorage::default();

    indexing::Pipeline::from_loader(FileLoader::new("./README.md"))
        .log_nodes()
        .then_chunk(transformers::ChunkMarkdown::from_chunk_range(100..512))
        .then(transformers::MetadataSummary::new(aws_bedrock.clone()))
        .then_store_with(memory_storage.clone())
        .log_all()
        .run()
        .await?;

    println!("Summaries:");
    println!(
        "{}",
        memory_storage
            .get_all_values()
            .await
            .iter()
            .filter_map(|n| n.metadata.get("Summary").map(|v| v.to_string()))
            .collect::<Vec<_>>()
            .join("\n---\n")
    );
    Ok(())
}
