//! Various transformers for chunking, embedding and transforming data
//!
//! These transformers are generic over their implementation and many require an
//! [`swiftide::integrations`] to be configured.
//!
//! Transformers that prompt have a default prompt configured. Prompts can be customized
//! and tailored, supporting Jinja style templating based on [`tera`]. See [`swiftide::prompt::Prompt`] and [`swiftide::prompt::PromptTemplate`]

#[cfg(feature = "tree-sitter")]
pub mod chunk_code;

#[cfg(feature = "tree-sitter")]
pub mod metadata_refs_defs_code;

#[cfg(feature = "tree-sitter")]
pub use chunk_code::ChunkCode;

#[cfg(feature = "tree-sitter")]
pub use metadata_refs_defs_code::MetadataRefsDefsCode;

pub mod chunk_markdown;
pub mod embed;
pub mod metadata_keywords;
pub mod metadata_qa_code;
pub mod metadata_qa_text;
pub mod metadata_summary;
pub mod metadata_title;

pub use chunk_markdown::ChunkMarkdown;
pub use embed::Embed;
pub use metadata_keywords::MetadataKeywords;
pub use metadata_qa_code::MetadataQACode;
pub use metadata_qa_text::MetadataQAText;
pub use metadata_summary::MetadataSummary;
pub use metadata_title::MetadataTitle;
