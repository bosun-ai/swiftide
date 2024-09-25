//! # [Swiftide] Indexing the Swiftide README with lots of metadata
//!
//! This example demonstrates how to index the Swiftide README with lots of metadata.
//!
//! The pipeline will:
//! - Load the README.md file from the current directory
//! - Chunk the file into pieces of 20 to 1024 bytes
//! - Generate questions and answers for each chunk
//! - Generate a summary for each chunk
//! - Generate a title for each chunk
//! - Generate keywords for each chunk
//! - Embed each chunk
//! - Store the nodes in Qdrant
//!
//! [Swiftide]: https://github.com/bosun-ai/swiftide
//! [examples]: https://github.com/bosun-ai/swiftide/blob/master/examples

use swiftide::{
    indexing,
    indexing::loaders::FileLoader,
    indexing::transformers::{
        ChunkMarkdown, Embed, MetadataKeywords, MetadataQAText, MetadataSummary, MetadataTitle,
    },
    integrations::{self, qdrant::Qdrant},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let openai_client = integrations::openai::OpenAI::builder()
        .default_embed_model("text-embedding-3-small")
        .default_prompt_model("gpt-4o")
        .build()?;

    indexing::Pipeline::from_loader(FileLoader::new("README.md").with_extensions(&["md"]))
        .with_concurrency(1)
        .then_chunk(ChunkMarkdown::from_chunk_range(20..2048))
        .then(MetadataQAText::new(openai_client.clone()))
        .then(MetadataSummary::new(openai_client.clone()))
        .then(MetadataTitle::new(openai_client.clone()))
        .then(MetadataKeywords::new(openai_client.clone()))
        .then_in_batch(Embed::new(openai_client.clone()))
        .log_all()
        .filter_errors()
        .then_store_with(
            Qdrant::builder()
                .batch_size(50)
                .vector_size(1536)
                .collection_name("swiftide-examples")
                .build()?,
        )
        .run()
        .await?;
    Ok(())
}
