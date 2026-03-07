//! # [Swiftide] Indexing the Swiftide README with lots of metadata
//!
//! This example demonstrates how to index the Swiftide README with lots of metadata.
//!
//! The pipeline will:
//! - Scrape the Bosun website
//! - Transform the html to markdown
//! - Chunk the markdown into smaller pieces
//! - Store the nodes in Memory
//!
//! [Swiftide]: https://github.com/bosun-ai/swiftide
//! [examples]: https://github.com/bosun-ai/swiftide/blob/master/examples
use spider::website::Website;
use swiftide::{
    indexing,
    indexing::persist::MemoryStorage,
    indexing::transformers::ChunkMarkdown,
    integrations::scraping::{HtmlToMarkdownTransformer, ScrapingLoader},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    indexing::Pipeline::from_loader(ScrapingLoader::from_spider(
        Website::new("https://www.bosun.ai/")
            .with_limit(1)
            .to_owned(),
    ))
    .then(HtmlToMarkdownTransformer::default())
    .then_chunk(ChunkMarkdown::from_chunk_range(20..2048))
    .log_all()
    .then_store_with(MemoryStorage::default())
    .run()
    .await?;
    Ok(())
}
