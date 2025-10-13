// show feature flags in the generated documentation
// https://doc.rust-lang.org/rustdoc/unstable-features.html#extensions-to-the-doc-attribute
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![doc(html_logo_url = "https://github.com/bosun-ai/swiftide/raw/master/images/logo.png")]
#![allow(unused_imports, reason = "that is what we do here")]
#![allow(clippy::doc_markdown, reason = "the readme is invalid and that is ok")]
#![doc = include_str!(env!("DOC_README"))]
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
    #[doc(inline)]
    pub use swiftide_core::tokenizer::*;
}

/// Abstractions for chat completions and LLM interactions.
#[doc(inline)]
pub use swiftide_core::chat_completion;

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

#[cfg(feature = "langfuse")]
#[doc(inline)]
pub use swiftide_langfuse as langfuse;

/// Re-exports for macros
#[doc(hidden)]
pub mod reexports {
    pub use ::anyhow;
    pub use ::async_trait;
    pub use ::schemars;
    pub use ::serde;
    pub use ::serde_json;
}
