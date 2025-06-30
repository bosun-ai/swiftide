// show feature flags in the generated documentation
// https://doc.rust-lang.org/rustdoc/unstable-features.html#extensions-to-the-doc-attribute
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![doc(html_logo_url = "https://github.com/bosun-ai/swiftide/raw/master/images/logo.png")]
#![allow(unused_imports, reason = "that is what we do here")]

//! Swiftide is a Rust library for building LLM applications, enabling fast data ingestion,
//! transformation, and indexing for effective querying and prompt injection, known as Retrieval
//! Augmented Generation. It provides flexible building blocks for creating various agents, allowing
//! rapid development from concept to production with minimal code.
//!
//! Part of the [bosun.ai](https://bosun.ai) project. An upcoming platform for autonomous code improvement.
//!
//! We <3 feedback: project ideas, suggestions, and complaints are very welcome. Feel free to open
//! an issue.
//!
//! Read more about the project on the [swiftide website](https://swiftide.rs)
//!
//! ### High level features
//!
//! - Build fast, streaming indexing and querying pipelines
//! - Easily build agents, mix and match with previously built pipelines
//! - A modular and extendable API, with minimal abstractions
//! - Integrations with popular LLMs and storage providers
//! - Ready to use pipeline transformations
//!
//! # Querying
//!
//! After running an indexing pipeline, you can use the [`query`] module to query the indexed data.
//!
//! # Examples
//!
//! ## Indexing markdown
//!
//! ```no_run
//! # use swiftide::indexing::loaders::FileLoader;
//! # use swiftide::indexing::transformers::{ChunkMarkdown, Embed, MetadataQAText};
//! # use swiftide::integrations::qdrant::Qdrant;
//! # use swiftide::integrations::openai::OpenAI;
//! # use swiftide::indexing::Pipeline;
//! # use anyhow::Result;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<()> {
//! # let qdrant_url = "url";
//! # let openai_client = OpenAI::builder().build()?;
//!  Pipeline::from_loader(FileLoader::new(".").with_extensions(&["md"]))
//!          .then_chunk(ChunkMarkdown::from_chunk_range(10..512))
//!          .then(MetadataQAText::new(openai_client.clone()))
//!          .then_in_batch(Embed::new(openai_client.clone()).with_batch_size(10))
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
//! ## Querying
//!
//! ```no_run
//! # use anyhow::Result;
//! # use swiftide::query::{query_transformers, self, response_transformers, answers};
//! # use swiftide::integrations::openai::OpenAI;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<()> {
//! # let qdrant_url = "url";
//! # let openai_client = OpenAI::builder().build()?;
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
//! ## Agents with tools
//!
//!
//! ```ignore
//! #[swiftide::tool(
//!     description = "Searches code",
//!     param(name = "code_query", description = "The code query")
//! )]
//! async fn search_code(
//!     context: &dyn AgentContext,
//!     code_query: &str,
//! ) -> Result<ToolOutput, ToolError> {
//!     let command_output = context
//!         .executor()
//!         .exec_cmd(&Command::shell(format!("rg '{code_query}'")))
//!         .await?;
//!
//!     Ok(command_output.into())
//! }
//!
//! agents::Agent::builder()
//!     .llm(&openai)
//!     .tools(vec![search_code()])
//!     .build()?
//!     .query("In what file can I find an example of a swiftide agent?")
//!     .await?;
//! ```
//! # Feature flags
//!
//! Swiftide has little features enabled by default, as there are some dependency heavy
//! integrations. You need to cherry-pick the tools and integrations you want to use.
#![doc = document_features::document_features!()]

#[doc(inline)]
pub use swiftide_core::prompt;
#[doc(inline)]
pub use swiftide_core::type_aliases::*;

#[cfg(feature = "swiftide-agents")]
#[doc(inline)]
pub use swiftide_agents as agents;

/// Common traits for common behaviour, re-exported from indexing and query
pub mod traits {
    #[doc(inline)]
    pub use swiftide_core::agent_traits::*;
    #[doc(inline)]
    pub use swiftide_core::chat_completion::traits::*;
    #[doc(inline)]
    pub use swiftide_core::indexing_traits::*;
    #[doc(inline)]
    pub use swiftide_core::query_traits::*;
}

pub mod chat_completion {
    #[doc(inline)]
    pub use swiftide_core::chat_completion::*;
}

/// Integrations with various platforms and external services.
pub mod integrations {
    #[doc(inline)]
    pub use swiftide_integrations::*;
}

/// This module serves as the main entry point for indexing in Swiftide.
///
/// The indexing system in Swiftide is designed to handle the asynchronous processing of large
/// volumes of data, including loading, transforming, and storing data chunks.
pub mod indexing {
    #[doc(inline)]
    pub use swiftide_core::indexing::*;
    #[doc(inline)]
    pub use swiftide_indexing::*;

    pub mod transformers {
        #[cfg(feature = "tree-sitter")]
        #[doc(inline)]
        pub use swiftide_integrations::treesitter::transformers::*;

        pub use swiftide_indexing::transformers::*;
    }
}

#[cfg(feature = "macros")]
#[doc(inline)]
pub use swiftide_macros::*;
/// # Querying pipelines
///
/// Swiftide allows you to define sophisticated query pipelines.
///
/// Consider the following code that uses Swiftide to load some markdown text, chunk it, embed it,
/// and store it in a Qdrant index:
///
/// ```no_run
/// use swiftide::{
///     indexing::{
///         self,
///         loaders::FileLoader,
///         transformers::{ChunkMarkdown, Embed, MetadataQAText},
///     },
///     integrations::{self, qdrant::Qdrant},
///     integrations::openai::OpenAI,
///     query::{self, answers, query_transformers, response_transformers},
/// };
///
/// async fn index() -> Result<(), Box<dyn std::error::Error>> {
///   let openai_client = OpenAI::builder()
///       .default_embed_model("text-embedding-3-large")
///       .default_prompt_model("gpt-4o")
///       .build()?;
///
///   let qdrant = Qdrant::builder()
///       .batch_size(50)
///       .vector_size(3072)
///       .collection_name("swiftide-examples")
///       .build()?;
///
///   indexing::Pipeline::from_loader(FileLoader::new("README.md"))
///       .then_chunk(ChunkMarkdown::from_chunk_range(10..2048))
///       .then(MetadataQAText::new(openai_client.clone()))
///       .then_in_batch(Embed::new(openai_client.clone()).with_batch_size(10))
///       .then_store_with(qdrant.clone())
///       .run()
///       .await?;
///
///   Ok(())
/// }
/// ```
///
/// We could then define a query pipeline that uses the Qdrant index to answer questions:
///
/// ```no_run
/// # use swiftide::{
/// #     indexing::{
/// #         self,
/// #         loaders::FileLoader,
/// #         transformers::{ChunkMarkdown, Embed, MetadataQAText},
/// #     },
/// #     integrations::{self, qdrant::Qdrant},
/// #     query::{self, answers, query_transformers, response_transformers},
/// #     integrations::openai::OpenAI,
/// # };
/// # async fn query() -> Result<(), Box<dyn std::error::Error>> {
/// #  let openai_client = OpenAI::builder()
/// #      .default_embed_model("text-embedding-3-large")
/// #      .default_prompt_model("gpt-4o")
/// #      .build()?;
/// #  let qdrant = Qdrant::builder()
/// #      .batch_size(50)
/// #      .vector_size(3072)
/// #      .collection_name("swiftide-examples")
/// #      .build()?;
/// // By default the search strategy is SimilaritySingleEmbedding
/// // which takes the latest query, embeds it, and does a similarity search
/// let pipeline = query::Pipeline::default()
///     .then_transform_query(query_transformers::GenerateSubquestions::from_client(
///         openai_client.clone(),
///     ))
///     .then_transform_query(query_transformers::Embed::from_client(
///         openai_client.clone(),
///     ))
///     .then_retrieve(qdrant.clone())
///     .then_transform_response(response_transformers::Summary::from_client(
///         openai_client.clone(),
///     ))
///     .then_answer(answers::Simple::from_client(openai_client.clone()));
///
/// let result = pipeline
///     .query("What is swiftide? Please provide an elaborate explanation")
///     .await?;
///
/// println!("{:?}", result.answer());
/// # Ok(())
/// # }
/// ```
///
/// By using a query pipeline to transform queries, we can improve the quality of the answers we get
/// from our index. In this example, we used an LLM to generate subquestions, embedding those and
/// then using them to search the index. Finally, we summarize the results and combine them together
/// into a single answer.
pub mod query {
    #[doc(inline)]
    pub use swiftide_core::querying::*;
    #[doc(inline)]
    pub use swiftide_query::*;
}

/// Re-exports for macros
#[doc(hidden)]
pub mod reexports {
    pub use ::anyhow;
    pub use ::async_trait;
    pub use ::serde;
    pub use ::serde_json;
}
