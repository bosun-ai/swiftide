/// This module serves as the entry point for various transformers used in the Swiftide project.
/// Swiftide is an asynchronous file ingestion and processing system used in RAG setups for efficient file processing.
///
/// The transformers provided here include functionalities for chunking code, chunking markdown, performing metadata QA on code and text,
/// and embedding using OpenAI's API. These transformers are essential components in the ingestion pipeline of Swiftide.
pub mod chunk_code;
pub mod chunk_markdown;
pub mod metadata_qa_code;
pub mod metadata_qa_text;
pub mod openai_embed;

/// Re-exporting the `ChunkCode` transformer.
///
/// `ChunkCode` is responsible for chunking code files into smaller, manageable pieces.
/// This is particularly useful for processing large codebases in a scalable manner.
pub use chunk_code::ChunkCode;

/// Re-exporting the `ChunkMarkdown` transformer.
///
/// `ChunkMarkdown` is responsible for chunking markdown files. This is useful for processing large markdown documents,
/// ensuring that they can be ingested and processed efficiently.
pub use chunk_markdown::ChunkMarkdown;

/// Re-exporting the `MetadataQACode` transformer.
///
/// `MetadataQACode` performs quality assurance on the metadata of code files.
/// This ensures that the metadata is accurate and complete, which is crucial for downstream processing tasks.
pub use metadata_qa_code::MetadataQACode;

/// Re-exporting the `MetadataQAText` transformer.
///
/// `MetadataQAText` performs quality assurance on the metadata of text files.
/// This ensures that the metadata is accurate and complete, which is crucial for downstream processing tasks.
pub use metadata_qa_text::MetadataQAText;

/// Re-exporting the `OpenAIEmbed` transformer.
///
/// `OpenAIEmbed` uses OpenAI's API to generate embeddings for the content.
/// These embeddings are useful for various NLP tasks, such as search and similarity comparisons.
pub use openai_embed::OpenAIEmbed;
