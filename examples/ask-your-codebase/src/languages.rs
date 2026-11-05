//! Which source files are indexed, and how each file is split into chunks.

use std::{fmt, ops::Range, path::Path};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use swiftide::{
    indexing::{IndexingStream, TextNode, transformers::ChunkCode},
    integrations::treesitter::SupportedLanguages,
    traits::ChunkerTransformer,
};

/// The programming languages to index, e.g. Rust and Python.
#[derive(Debug, Clone)]
pub struct SourceLanguages(Vec<SupportedLanguages>);

impl SourceLanguages {
    pub fn new(languages: Vec<SupportedLanguages>) -> Self {
        Self(languages)
    }

    /// File extensions (without the dot) of all languages, e.g. `["rs", "py"]`.
    pub fn extensions(&self) -> Vec<&str> {
        self.0
            .iter()
            .flat_map(|language| language.file_extensions().iter().copied())
            .collect()
    }
}

impl fmt::Display for SourceLanguages {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<String> = self.0.iter().map(ToString::to_string).collect();
        f.write_str(&names.join(", "))
    }
}

// ANCHOR: chunker
/// Splits each file with a tree-sitter `ChunkCode` for the file's language.
///
/// `ChunkCode` is built for a single language; this chunker picks the right one per file from
/// its extension, so one pipeline can index a polyglot codebase.
#[derive(Clone)]
pub struct ChunkByLanguage {
    chunkers: Vec<(SupportedLanguages, ChunkCode)>,
}

impl ChunkByLanguage {
    /// Chunks are between `chunk_size.start` and `chunk_size.end` bytes; smaller ones are dropped.
    pub fn try_new(languages: &SourceLanguages, chunk_size: Range<usize>) -> Result<Self> {
        let chunkers = languages
            .0
            .iter()
            .map(|&language| {
                let chunker =
                    ChunkCode::try_for_language_and_chunk_size(language, chunk_size.clone())?;
                Ok((language, chunker))
            })
            .collect::<Result<_>>()?;

        Ok(Self { chunkers })
    }

    fn chunker_for(&self, path: &Path) -> Option<&ChunkCode> {
        let extension = path.extension()?.to_str()?;

        self.chunkers
            .iter()
            .find(|(language, _)| language.file_extensions().contains(&extension))
            .map(|(_, chunker)| chunker)
    }
}

#[async_trait]
impl ChunkerTransformer for ChunkByLanguage {
    type Input = String;
    type Output = String;

    async fn transform_node(&self, node: TextNode) -> IndexingStream<String> {
        match self.chunker_for(&node.path) {
            Some(chunker) => chunker.transform_node(node).await,
            None => anyhow!("No chunker for {}", node.path.display()).into(),
        }
    }
}
// ANCHOR_END: chunker
