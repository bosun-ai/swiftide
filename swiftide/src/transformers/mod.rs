//! Various transformers for chunking, embedding and transforming data
//!
//! These transformers are generic over their implementation. In most cases you will also need
//! to enable integrations.

#[cfg(feature = "tree-sitter")]
pub mod chunk_code;

#[cfg(feature = "tree-sitter")]
pub mod file_to_context_tree_sitter;

pub mod chunk_markdown;
pub mod contextualize_code_chunk;
pub mod embed;
pub mod metadata_keywords;
pub mod metadata_qa_code;
pub mod metadata_qa_text;
pub mod metadata_summary;
pub mod metadata_title;

#[cfg(feature = "tree-sitter")]
pub use chunk_code::ChunkCode;

#[cfg(feature = "tree-sitter")]
pub use file_to_context_tree_sitter::FileToContextTreeSitter;

pub use chunk_markdown::ChunkMarkdown;
pub use contextualize_code_chunk::ContextualizeCodeChunk;
pub use embed::Embed;
pub use metadata_keywords::MetadataKeywords;
pub use metadata_qa_code::MetadataQACode;
pub use metadata_qa_text::MetadataQAText;
pub use metadata_summary::MetadataSummary;
pub use metadata_title::MetadataTitle;
