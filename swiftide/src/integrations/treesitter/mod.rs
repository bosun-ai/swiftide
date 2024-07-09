//! Chunking code with tree-sitter
mod splitter;
mod summarizer;
mod supported_languages;

pub use splitter::{ChunkSize, CodeSplitter, CodeSplitterBuilder};
pub use summarizer::{CodeSummarizer, CodeSummarizerBuilder};
pub use supported_languages::SupportedLanguages;
