//! Chunking code with tree-sitter and various tools
mod code_tree;
mod outliner;
mod queries;
mod splitter;
mod supported_languages;

pub use code_tree::{CodeParser, CodeTree, ReferencesAndDefinitions};
pub use outliner::{CodeOutliner, CodeOutlinerBuilder};
pub use splitter::{ChunkSize, CodeSplitter, CodeSplitterBuilder};
pub use supported_languages::SupportedLanguages;

pub mod chunk_code;
pub mod compress_code_outline;
pub mod metadata_qa_code;
pub mod metadata_refs_defs_code;
pub mod outline_code_tree_sitter;

pub mod transformers {
    pub use super::chunk_code::{self, ChunkCode};
    pub use super::compress_code_outline::{self, CompressCodeOutline};
    pub use super::metadata_qa_code::{self, MetadataQACode};
    pub use super::metadata_refs_defs_code::{self, MetadataRefsDefsCode};
    pub use super::outline_code_tree_sitter::{self, OutlineCodeTreeSitter};
}
