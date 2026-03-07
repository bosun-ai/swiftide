//! Various transformers for chunking, embedding and transforming data
//!
//! These transformers are generic over their implementation and many require a
//! swiftide integration to be configured.
//!
//! Transformers that prompt have a default prompt configured. Prompts can be customized
//! and tailored, supporting Jinja style templating based on [tera](https://docs.rs/tera/latest/tera/).
//!
//!  See [`swiftide_core::prompt::Prompt`] and [`swiftide_core::template::Template`]

pub mod chunk_markdown;
pub mod chunk_text;
pub mod embed;
pub mod metadata_keywords;
pub mod metadata_qa_text;
pub mod metadata_summary;
pub mod metadata_title;
pub mod sparse_embed;

pub use chunk_markdown::ChunkMarkdown;
pub use chunk_text::ChunkText;
pub use embed::Embed;
pub use metadata_keywords::MetadataKeywords;
pub use metadata_qa_text::MetadataQAText;
pub use metadata_summary::MetadataSummary;
pub use metadata_title::MetadataTitle;
pub use sparse_embed::SparseEmbed;
