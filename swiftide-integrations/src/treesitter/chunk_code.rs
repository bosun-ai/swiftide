//! Chunk code using tree-sitter
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use derive_builder::Builder;

use crate::treesitter::{ChunkSize, CodeSplitter, SupportedLanguages};
use swiftide_core::{
    indexing::{IndexingStream, Node},
    ChunkerTransformer,
};

/// The `ChunkCode` struct is responsible for chunking code into smaller pieces
/// based on the specified language and chunk size.
///
/// It uses tree-sitter under the hood, and tries to split the code into smaller, meaningful
/// chunks.
///
/// # Example
///
/// ```no_run
/// # use swiftide_integrations::treesitter::transformers::ChunkCode;
/// # use swiftide_integrations::treesitter::SupportedLanguages;
/// // Chunk rust code with a maximum chunk size of 1000 bytes.
/// ChunkCode::try_for_language_and_chunk_size(SupportedLanguages::Rust, 1000);
///
/// // Chunk python code with a minimum chunk size of 500 bytes and maximum chunk size of 2048.
/// // Smaller chunks than 500 bytes will be discarded.
/// ChunkCode::try_for_language_and_chunk_size(SupportedLanguages::Python, 500..2048);
/// ````
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
    /// - `lang`: The programming language to be used for chunking. It should implement
    ///   `TryInto<SupportedLanguages>`.
    ///
    /// # Returns
    /// - `Result<Self>`: Returns an instance of `ChunkCode` if successful, otherwise returns an
    ///   error.
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
    /// - `lang`: The programming language to be used for chunking. It should implement
    ///   `TryInto<SupportedLanguages>`.
    /// - `chunk_size`: The size of the chunks. It should implement `Into<ChunkSize>`.
    ///
    /// # Returns
    /// - `Result<Self>`: Returns an instance of `ChunkCode` if successful, otherwise returns an
    ///   error.
    ///
    /// # Errors
    /// - Returns an error if the language is not supported, if the chunk size is invalid, or if the
    ///   `CodeSplitter` fails to build.
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
    /// Transforms a `Node` by splitting its code chunk into smaller pieces.
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
            let mut offset = 0;

            IndexingStream::iter(split.into_iter().map(move |chunk| {
                let chunk_size = chunk.len();

                let node = Node::build_from_other(&node)
                    .chunk(chunk)
                    .offset(offset)
                    .build();

                offset += chunk_size;

                node
            }))
        } else {
            // Send the error downstream
            IndexingStream::iter(vec![Err(split_result
                .with_context(|| format!("Failed to chunk {}", node.path.display()))
                .unwrap_err())])
        }
    }

    fn concurrency(&self) -> Option<usize> {
        self.concurrency
    }
}
