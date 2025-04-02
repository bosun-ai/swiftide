// show feature flags in the generated documentation
// https://doc.rust-lang.org/rustdoc/unstable-features.html#extensions-to-the-doc-attribute
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![doc(html_logo_url = "https://github.com/bosun-ai/swiftide/raw/master/images/logo.png")]

//! Integrations with various platforms and external services.

#[cfg(feature = "anthropic")]
pub mod anthropic;
#[cfg(feature = "aws-bedrock")]
pub mod aws_bedrock;
#[cfg(feature = "dashscope")]
pub mod dashscope;
#[cfg(feature = "duckdb")]
pub mod duckdb;
#[cfg(feature = "fastembed")]
pub mod fastembed;
#[cfg(feature = "fluvio")]
pub mod fluvio;
#[cfg(feature = "groq")]
pub mod groq;
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
