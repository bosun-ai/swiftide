//! Integrations with various platforms and external services.

#[cfg(feature = "aws-bedrock")]
pub mod aws_bedrock;
#[cfg(feature = "fastembed")]
pub mod fastembed;
#[cfg(feature = "fluvio")]
pub mod fluvio;
#[cfg(feature = "groq")]
pub mod groq;
#[cfg(feature = "ollama")]
pub mod ollama;
#[cfg(feature = "openai")]
pub mod openai;
#[cfg(feature = "qdrant")]
pub mod qdrant;
#[cfg(feature = "redis")]
pub mod redis;
#[cfg(feature = "scraping")]
pub mod scraping;
#[cfg(feature = "tree-sitter")]
pub mod treesitter;
