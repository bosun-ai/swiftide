//! # [Swiftide] Ingesting file with multiple metadata stored as named vectors
//!
//! This example demonstrates how to ingest a LICENSE file, generate multiple metadata, and store it
//! all in Qdrant with individual named vectors
//!
//! The pipeline will:
//! - Load the LICENSE file from the current directory
//! - Chunk the file into pieces of 20 to 1024 bytes
//! - Generate questions and answers for each chunk
//! - Generate a summary for each chunk
//! - Generate a title for each chunk
//! - Generate keywords for each chunk
//! - Embed each chunk
//! - Embed each metadata
//! - Store the nodes in Qdrant with chunk and metadata embeds as named vectors
//!
//! [Swiftide]: https://github.com/bosun-ai/swiftide
//! [examples]: https://github.com/bosun-ai/swiftide/blob/master/examples

use swiftide::{
    indexing::loaders::FileLoader,
    indexing::transformers::{
        metadata_keywords, metadata_qa_text, metadata_summary, metadata_title, ChunkMarkdown,
        Embed, MetadataKeywords, MetadataQAText, MetadataSummary, MetadataTitle,
    },
    indexing::{self, EmbedMode, EmbeddedField},
    integrations::{
        self,
        qdrant::{Distance, Qdrant, VectorConfig},
    },
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let openai_client = integrations::openai::OpenAI::builder()
        .default_embed_model("text-embedding-3-small")
        .default_prompt_model("gpt-4o")
        .build()?;

    indexing::Pipeline::from_loader(FileLoader::new("LICENSE"))
        .with_concurrency(1)
        .with_embed_mode(EmbedMode::PerField)
        .then_chunk(ChunkMarkdown::from_chunk_range(20..2048))
        .then(MetadataQAText::new(openai_client.clone()))
        .then(MetadataSummary::new(openai_client.clone()))
        .then(MetadataTitle::new(openai_client.clone()))
        .then(MetadataKeywords::new(openai_client.clone()))
        .then_in_batch(Embed::new(openai_client.clone()).with_batch_size(10))
        .log_all()
        .filter_errors()
        .then_store_with(
            Qdrant::builder()
                .batch_size(50)
                .vector_size(1536)
                .collection_name("swiftide-multi-vectors")
                .with_vector(EmbeddedField::Chunk)
                .with_vector(EmbeddedField::Metadata(metadata_qa_text::NAME.into()))
                .with_vector(EmbeddedField::Metadata(metadata_summary::NAME.into()))
                .with_vector(
                    VectorConfig::builder()
                        .embedded_field(EmbeddedField::Metadata(metadata_title::NAME.into()))
                        .distance(Distance::Manhattan)
                        .build()?,
                )
                .with_vector(EmbeddedField::Metadata(metadata_keywords::NAME.into()))
                .build()?,
        )
        .run()
        .await?;
    Ok(())
}
