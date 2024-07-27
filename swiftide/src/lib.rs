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
//! - Jinja-like templating for prompts
//! - Store into multiple backends
//! - `tracing` supported for logging and tracing, see /examples and the `tracing` crate for more information.
//!
//! ## Querying
//!
//! We are working on an experimental query pipeline, which you can find in [`swiftide::query`]
//!
//! ## Examples
//!
//! ### Indexing markdown
//!
//! ```no_run
//! # use swiftide::indexing::loaders::FileLoader;
//! # use swiftide::indexing::transformers::{ChunkMarkdown, Embed, MetadataQAText};
//! # use swiftide::integrations::qdrant::Qdrant;
//! # use swiftide::indexing::Pipeline;
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
//! ### Experimental querying
//!
//! ```no_run
//! # use anyhow::Result;
//! # use swiftide::query::{query_transformers, self, response_transformers, answers};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<()> {
//! # let qdrant_url = "url";
//! # let openai_client = swiftide::integrations::openai::OpenAI::builder().build()?;
//! # let qdrant = swiftide::integrations::qdrant::Qdrant::try_from_url(qdrant_url)?
//! #                .batch_size(50)
//! #                .vector_size(1536)
//! #                .collection_name("swiftide-examples".to_string())
//! #                .build()?;
//! query::Pipeline::default()
//!     .then_transform_query(query_transformers::GenerateSubquestions::from_client(
//!         openai_client.clone(),
//!     ))
//!     .then_transform_query(query_transformers::Embed::from_client(
//!         openai_client.clone(),
//!     ))
//!     .then_retrieve(qdrant.clone())
//!     .then_transform_response(response_transformers::Summary::from_client(
//!         openai_client.clone(),
//!     ))
//!     .then_answer(answers::Simple::from_client(openai_client.clone()))
//!     .query("What is swiftide?")
//!     .await?;
//! # Ok(())
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

#[doc(inline)]
pub use swiftide_core::prompt;
#[doc(inline)]
pub use swiftide_core::type_aliases::*;

/// Common traits for common behaviour, re-exported from indexing and query
pub mod traits {
    #[doc(inline)]
    pub use swiftide_core::indexing_traits::*;
    #[doc(inline)]
    pub use swiftide_core::query_traits::*;
}

/// Integrations with various platforms and external services.
pub mod integrations {
    #[doc(inline)]
    pub use swiftide_integrations::*;
}

/// This module serves as the main entry point for indexing in Swiftide.
///
/// The indexing system in Swiftide is designed to handle the asynchronous processing of large volumes
/// of data, including loading, transforming, and storing data chunks.
pub mod indexing {
    #[doc(inline)]
    pub use swiftide_core::indexing::*;
    #[doc(inline)]
    pub use swiftide_indexing::*;
}

/// Query your indexed data with a transforming pipeline
pub mod query {
    #[doc(inline)]
    pub use swiftide_core::querying::*;
    #[doc(inline)]
    pub use swiftide_query::*;
}

#[doc(hidden)]
#[cfg(feature = "test-utils")]
pub mod test_utils;
