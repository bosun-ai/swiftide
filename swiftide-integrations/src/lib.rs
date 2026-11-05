//! Integrations with LLM providers, embedding models, storage backends and other services.
//!
//! Every integration sits behind its own cargo feature (for example `openai`, `anthropic`,
//! `ollama`, `qdrant`, `pgvector`, `lancedb`, `redis`, `tree-sitter`, `fastembed`, `scraping`),
//! so only what you enable gets compiled. Each one implements the traits from `swiftide-core`,
//! such as `ChatCompletion`, `SimplePrompt`, `EmbeddingModel`, `Persist`, `Retrieve` or `Loader`,
//! which makes it usable from indexing pipelines, query pipelines and agents alike.
//!
//! The [`swiftide`](https://docs.rs/swiftide) facade re-exports this crate as
//! `swiftide::integrations` and forwards the same feature names.
//!
//! Guides: [integrations](https://swiftide.rs/integrations/) and
//! [models](https://swiftide.rs/models/).
//!
//! ```no_run
//! # #[cfg(feature = "openai")]
//! # fn build() -> anyhow::Result<()> {
//! let openai_client = swiftide_integrations::openai::OpenAI::builder()
//!     .default_embed_model("text-embedding-3-small")
//!     .default_prompt_model("gpt-4o")
//!     .build()?;
//! # Ok(())
//! # }
//! ```
// show feature flags in the generated documentation
// https://doc.rust-lang.org/rustdoc/unstable-features.html#extensions-to-the-doc-attribute
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, doc(auto_cfg))]
#![doc(html_logo_url = "https://github.com/bosun-ai/swiftide/raw/master/images/logo.png")]

#[cfg(feature = "anthropic")]
pub mod anthropic;
#[cfg(feature = "aws-bedrock")]
pub mod aws_bedrock_v2;
#[cfg(feature = "dashscope")]
pub mod dashscope;
#[cfg(feature = "duckdb")]
pub mod duckdb;
#[cfg(feature = "fastembed")]
pub mod fastembed;
#[cfg(feature = "fluvio")]
pub mod fluvio;
#[cfg(feature = "gemini")]
pub mod gemini;
#[cfg(feature = "groq")]
pub mod groq;
#[cfg(feature = "kafka")]
pub mod kafka;
#[cfg(feature = "lancedb")]
pub mod lancedb;
#[cfg(feature = "ollama")]
pub mod ollama;
#[cfg(feature = "open-router")]
pub mod open_router;
#[cfg(feature = "openai")]
pub mod openai;
#[cfg(feature = "parquet")]
pub mod parquet;
#[cfg(feature = "pgvector")]
pub mod pgvector;
#[cfg(feature = "qdrant")]
pub mod qdrant;
#[cfg(feature = "redb")]
pub mod redb;
#[cfg(feature = "redis")]
pub mod redis;
#[cfg(feature = "scraping")]
pub mod scraping;
#[cfg(feature = "tiktoken")]
pub mod tiktoken;
#[cfg(feature = "tree-sitter")]
pub mod treesitter;
