//! Chunk code using tree-sitter
use anyhow::Result;
use async_trait::async_trait;
use derive_builder::Builder;

use crate::{
    indexing::{IndexingStream, Node},
    ChunkerTransformer,
};
use swiftide_integrations::treesitter::{ChunkSize, CodeSplitter, SupportedLanguages};

/// The `ChunkCode` struct is responsible for chunking code into smaller pieces
/// based on the specified language and chunk size. This is a crucial step in the
/// indexing pipeline for processing and embedding code efficiently.
#[derive(Debug, Clone, Builder)]
#[builder(pattern = "owned", setter(into, strip_option))]
pub struct ChunkCode {
    chunker: CodeSplitter,
    #[builder(default)]
    concurrency: Option<usize>,
}

impl ChunkCode {
    pub fn builder() -> ChunkCodeBuilder {
        ChunkCodeBuilder::default()
    }

    /// Tries to create a `ChunkCode` instance for a given programming language.
    ///
    /// # Parameters
    /// - `lang`: The programming language to be used for chunking. It should implement `TryInto<SupportedLanguages>`.
    ///
    /// # Returns
    /// - `Result<Self>`: Returns an instance of `ChunkCode` if successful, otherwise returns an error.
    ///
    /// # Errors
    /// - Returns an error if the language is not supported or if the `CodeSplitter` fails to build.
    pub fn try_for_language(lang: impl TryInto<SupportedLanguages>) -> Result<Self> {
        Ok(Self {
            chunker: CodeSplitter::builder().try_language(lang)?.build()?,
            concurrency: None,
        })
    }

    /// Tries to create a `ChunkCode` instance for a given programming language and chunk size.
    ///
    /// # Parameters
    /// - `lang`: The programming language to be used for chunking. It should implement `TryInto<SupportedLanguages>`.
    /// - `chunk_size`: The size of the chunks. It should implement `Into<ChunkSize>`.
    ///
    /// # Returns
    /// - `Result<Self>`: Returns an instance of `ChunkCode` if successful, otherwise returns an error.
    ///
    /// # Errors
    /// - Returns an error if the language is not supported, if the chunk size is invalid, or if the `CodeSplitter` fails to build.
    pub fn try_for_language_and_chunk_size(
        lang: impl TryInto<SupportedLanguages>,
        chunk_size: impl Into<ChunkSize>,
    ) -> Result<Self> {
        Ok(Self {
            chunker: CodeSplitter::builder()
                .try_language(lang)?
                .chunk_size(chunk_size)
                .build()?,
            concurrency: None,
        })
    }

    #[must_use]
    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = Some(concurrency);
        self
    }
}

#[async_trait]
impl ChunkerTransformer for ChunkCode {
    /// Transforms an `Node` by splitting its code chunk into smaller pieces.
    ///
    /// # Parameters
    /// - `node`: The `Node` containing the code chunk to be split.
    ///
    /// # Returns
    /// - `IndexingStream`: A stream of `Node` instances, each containing a smaller chunk of code.
    ///
    /// # Errors
    /// - If the code splitting fails, an error is sent downstream.
    #[tracing::instrument(skip_all, name = "transformers.chunk_code")]
    async fn transform_node(&self, node: Node) -> IndexingStream {
        let split_result = self.chunker.split(&node.chunk);

        if let Ok(split) = split_result {
            IndexingStream::iter(split.into_iter().map(move |chunk| {
                Ok(Node {
                    chunk,
                    ..node.clone()
                })
            }))
        } else {
            // Send the error downstream
            IndexingStream::iter(vec![Err(split_result.unwrap_err())])
        }
    }

    fn concurrency(&self) -> Option<usize> {
        self.concurrency
    }
}
