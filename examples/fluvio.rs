//! # [Swiftide] Loading data from Fluvio
//!
//! This example demonstrates how to index the Swiftide codebase itself.
//! Note that for it to work correctly you need to have OPENAI_API_KEY set, redis and qdrant
//! running.
//!
//! The pipeline will:
//! - Load all `.rs` files from the current directory
//! - Skip any nodes previously processed; hashes are based on the path and chunk (not the
//!   metadata!)
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
    indexing::{self, transformers::Embed},
    integrations::{
        fastembed::FastEmbed,
        fluvio::{ConsumerConfigExt, Fluvio},
        qdrant::Qdrant,
    },
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    static TOPIC_NAME: &str = "hello-rust";
    static PARTITION_NUM: u32 = 0;

    let loader = Fluvio::builder()
        .consumer_config_ext(
            ConsumerConfigExt::builder()
                .topic(TOPIC_NAME)
                .partition(PARTITION_NUM)
                .offset_start(fluvio::Offset::from_end(1))
                .build()
                .unwrap(),
        )
        .build()
        .unwrap();

    indexing::Pipeline::from_loader(loader)
        .then_in_batch(Embed::new(FastEmbed::try_default().unwrap()).with_batch_size(10))
        .then_store_with(
            Qdrant::builder()
                .batch_size(50)
                .vector_size(384)
                .collection_name("swiftide-examples")
                .build()?,
        )
        .run()
        .await?;
    Ok(())
}
