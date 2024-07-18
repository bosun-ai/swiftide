//! Chunking code with tree-sitter and various tools
mod code_tree;
mod queries;
mod splitter;
mod supported_languages;

pub use code_tree::{CodeParser, CodeTree, ReferencesAndDefinitions};
pub use splitter::{ChunkSize, CodeSplitter, CodeSplitterBuilder};
pub use supported_languages::SupportedLanguages;
