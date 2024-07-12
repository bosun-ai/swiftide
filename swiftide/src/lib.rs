//! # Swiftide
//!
//! Swiftide is a data indexing and processing library, tailored for Retrieval Augmented Generation (RAG). When building applications with large language models (LLM), these LLMs need access to external resources. Data needs to be transformed, enriched, split up, embedded, and persisted. It is build in Rust, using parallel, asynchronous streams and is blazingly fast.
//!
//! Part of the [bosun.ai](https://bosun.ai) project. An upcoming platform for autonomous code improvement.
//!
//! We <3 feedback: project ideas, suggestions, and complaints are very welcome. Feel free to open an issue.
//!
//! Read more about the project on the [swiftide website](https://swiftide.rs)
//!
//! ## Features
//!
//! - Extremely fast streaming indexing pipeline with async, parallel processing
//! - Integrations with `OpenAI`, `Redis`, `Qdrant`, `FastEmbed`, `Treesitter` and more
//! - A variety of loaders, transformers, and embedders and other common, generic tools
//! - Bring your own transformers by extending straightforward traits
//! - Splitting and merging pipelines
//! - Store into multiple backends
//! - `tracing` supported for logging and tracing, see /examples and the `tracing` crate for more information.
//!
//! ## Example
//!
//! ```no_run
//! use swiftide::loaders::FileLoader;
//! use swiftide::transformers::{ChunkMarkdown, Embed, MetadataQAText};
//! use swiftide::integrations::qdrant::Qdrant;
//! use swiftide::indexing::Pipeline;
//! # use anyhow::Result;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<()> {
//! # let qdrant_url = "url";
//! # let openai_client = swiftide::integrations::openai::OpenAI::builder().build()?;
//!  Pipeline::from_loader(FileLoader::new(".").with_extensions(&["md"]))
//!          .then_chunk(ChunkMarkdown::from_chunk_range(10..512))
//!          .then(MetadataQAText::new(openai_client.clone()))
//!          .then_in_batch(10, Embed::new(openai_client.clone()))
//!          .then_store_with(
//!              Qdrant::try_from_url(qdrant_url)?
//!                  .batch_size(50)
//!                  .vector_size(1536)
//!                  .collection_name("swiftide-examples".to_string())
//!                  .build()?,
//!          )
//!          .run()
//!          .await
//! # }
//! ```
//!
//! ## Feature flags
//!
//! Swiftide has little features enabled by default as there are some dependency heavy
//! integrations.
//!
//! Either use the 'all' feature flag (not recommended), or enable the integrations that you need.
//! Each integration has a similarly named feature flag.

pub mod indexing;
pub mod integrations;
pub mod loaders;
pub mod persist;
pub mod prompt;
pub mod traits;
pub mod transformers;
pub mod type_aliases;

pub use traits::*;
pub use type_aliases::*;

/// Deprecated re-export of `indexing`, use that instead.
#[deprecated(
    since = "0.6.0",
    note = "Renamed references of Indexing to Indexing for more appropriate naming. Will be removed in a future release."
)]
pub mod ingestion {
    pub use crate::indexing::*;

    pub use crate::indexing::IndexingStream;
    pub use crate::indexing::Node;
    pub use crate::indexing::Pipeline;
}
