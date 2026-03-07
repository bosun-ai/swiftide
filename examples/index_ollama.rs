//! # [Swiftide] Indexing with Ollama
//!
//! This example demonstrates how to index the Swiftide codebase itself.
//! Note that for it to work correctly you need to have ollama running on the default local port.
//!
//! The pipeline will:
//! - Loads the readme from the project
//! - Chunk the code into pieces of 10 to 2048 bytes
//! - Run metadata QA on each chunk with Ollama; generating questions and answers and adding
//!   metadata
//! - Embed the chunks in batches of 10, Metadata is embedded by default
//! - Store the nodes in Memory Storage
//!
//! [Swiftide]: https://github.com/bosun-ai/swiftide
//! [examples]: https://github.com/bosun-ai/swiftide/blob/master/examples

use swiftide::{
    indexing,
    indexing::loaders::FileLoader,
    indexing::persist::MemoryStorage,
    indexing::transformers::{ChunkMarkdown, Embed, MetadataQAText},
    integrations,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let ollama_client = integrations::ollama::Ollama::default()
        .with_default_prompt_model("llama3.1")
        .to_owned();

    let fastembed = integrations::fastembed::FastEmbed::try_default()?;
    let memory_store = MemoryStorage::default();

    indexing::Pipeline::from_loader(FileLoader::new("README.md"))
        .then_chunk(ChunkMarkdown::from_chunk_range(10..2048))
        .then(MetadataQAText::new(ollama_client.clone()))
        .then_in_batch(Embed::new(fastembed).with_batch_size(10))
        .then_store_with(memory_store.clone())
        .run()
        .await?;

    println!("Example results:");
    println!(
        "{}",
        memory_store
            .get_all_values()
            .await
            .into_iter()
            .flat_map(|n| n.metadata.into_values().map(|v| v.to_string()))
            .collect::<Vec<_>>()
            .join("\n")
    );
    Ok(())
}
