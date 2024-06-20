//! # [Swiftide] Ingesting the Swiftide itself example
//!
//! This example demonstrates how to ingest the Swiftide codebase itself.
//! Note that for it to work correctly you need to have OPENAI_API_KEY set, redis and qdrant
//! running.
//!
//! The pipeline will:
//! - Load all `.rs` files from the current directory
//! - Skip any nodes previously processed; hashes are based on the path and chunk (not the
//! metadata!)
//! - Run metadata QA on each chunk; generating questions and answers and adding metadata
//! - Chunk the code into pieces of 10 to 2048 bytes
//! - Embed the chunks in batches of 10, Metadata is embedded by default
//! - Store the nodes in Qdrant
//!
//! Note that metadata is copied over to smaller chunks when chunking. When making LLM requests
//! with lots of small chunks, consider the rate limits of the API.
//!
//! [Swiftide]: https://github.com/bosun-ai/swiftide
//! [examples]: https://github.com/bosun-ai/swiftide/blob/master/examples

use swiftide::{
    ingestion,
    integrations::{self, qdrant::Qdrant, redis::RedisNodeCache},
    loaders::FileLoader,
    transformers::{ChunkCode, Embed, MetadataQACode},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let openai_client = integrations::openai::OpenAI::builder()
        .default_embed_model("text-embedding-3-small")
        .default_prompt_model("gpt-3.5-turbo")
        .build()?;

    let redis_url = std::env::var("REDIS_URL")
        .as_deref()
        .unwrap_or("redis://localhost:6379")
        .to_owned();

    let qdrant_url = std::env::var("QDRANT_URL")
        .as_deref()
        .unwrap_or("http://localhost:6334")
        .to_owned();

    ingestion::IngestionPipeline::from_loader(FileLoader::new(".").with_extensions(&["rs"]))
        .filter_cached(RedisNodeCache::try_from_url(
            redis_url,
            "swiftide-examples",
        )?)
        .then(MetadataQACode::new(openai_client.clone()))
        .then_chunk(ChunkCode::try_for_language_and_chunk_size(
            "rust",
            10..2048,
        )?)
        .then_in_batch(10, Embed::new(openai_client.clone()))
        .then_store_with(
            Qdrant::try_from_url(qdrant_url)?
                .batch_size(50)
                .vector_size(1536)
                .collection_name("swiftide-examples".to_string())
                .build()?,
        )
        .run()
        .await?;
    Ok(())
}
