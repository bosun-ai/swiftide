//! # [Swiftide] Indexing the Swiftide itself example with reduced context size
//!
//! This example demonstrates how to index the Swiftide codebase itself, optimizing for a smaller
//! context size. Note that for it to work correctly you need to have OPENAI_API_KEY set, redis and
//! qdrant running.
//!
//! The pipeline will:
//! - Load all `.rs` files from the current directory
//! - Skip any nodes previously processed; hashes are based on the path and chunk (not the
//!   metadata!)
//! - Generate an outline of the symbols defined in each file to be used as context in a later step
//!   and store it in the metadata
//! - Chunk the code into pieces of 10 to 2048 bytes
//! - For each chunk, generate a condensed subset of the symbols outline tailored for that specific
//!   chunk and store that in the metadata
//! - Run metadata QA on each chunk; generating questions and answers and adding metadata
//! - Embed the chunks in batches of 10, Metadata is embedded by default
//! - Store the nodes in Qdrant
//!
//! Note that metadata is copied over to smaller chunks when chunking. When making LLM requests
//! with lots of small chunks, consider the rate limits of the API.
//!
//! [Swiftide]: https://github.com/bosun-ai/swiftide
//! [examples]: https://github.com/bosun-ai/swiftide/blob/master/examples

use swiftide::indexing;
use swiftide::indexing::loaders::FileLoader;
use swiftide::indexing::transformers::{ChunkCode, Embed, MetadataQACode};
use swiftide::integrations::{self, qdrant::Qdrant, redis::Redis};

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

    let chunk_size = 2048;

    indexing::Pipeline::from_loader(FileLoader::new(".").with_extensions(&["rs"]))
        .filter_cached(Redis::try_from_url(
            redis_url,
            "swiftide-examples-codebase-reduced-context",
        )?)
        .then(
            indexing::transformers::OutlineCodeTreeSitter::try_for_language(
                "rust",
                Some(chunk_size),
            )?,
        )
        .then(MetadataQACode::new(openai_client.clone()))
        .then_chunk(ChunkCode::try_for_language_and_chunk_size(
            "rust",
            10..chunk_size,
        )?)
        .then(indexing::transformers::CompressCodeOutline::new(
            openai_client.clone(),
        ))
        .then_in_batch(Embed::new(openai_client.clone()).with_batch_size(10))
        .then_store_with(
            Qdrant::builder()
                .batch_size(50)
                .vector_size(1536)
                .collection_name("swiftide-examples-codebase-reduced-context")
                .build()?,
        )
        .run()
        .await?;
    Ok(())
}
