//! Chunk code using tree-sitter
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use derive_builder::Builder;

use crate::treesitter::{ChunkSize, CodeSplitter, SupportedLanguages};
use swiftide_core::{
    ChunkerTransformer,
    indexing::{IndexingStream, TextNode},
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
    type Input = String;
    type Output = String;
    /// Transforms a `TextNode` by splitting its code chunk into smaller pieces.
    ///
    /// # Parameters
    /// - `node`: The `TextNode` containing the code chunk to be split.
    ///
    /// # Returns
    /// - `IndexingStream`: A stream of `TextNode` instances, each containing a smaller chunk of
    ///   code.
    ///
    /// # Errors
    /// - If the code splitting fails, an error is sent downstream.
    #[tracing::instrument(skip_all, name = "transformers.chunk_code")]
    async fn transform_node(&self, node: TextNode) -> IndexingStream<String> {
        let split_result = self.chunker.split(&node.chunk);

        if let Ok(split) = split_result {
            let mut offset = 0;

            IndexingStream::iter(node.into_chunks(split).map(move |mut chunk| {
                let chunk_size = chunk.chunk.len();
                chunk.offset = offset;
                offset += chunk_size;
                Ok(chunk)
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

#[cfg(test)]
mod tests {
    use futures_util::stream::TryStreamExt;
    use swiftide_core::{ChunkerTransformer, indexing::Metadata};

    use super::*;

    #[tokio::test]
    async fn preserves_code_chunks_offsets_and_first_ancestor() {
        let code = "fn alpha() { println!(\"α\"); }\nfn beta() { println!(\"β\"); }\nfn gamma() { println!(\"γ\"); }";
        let transformer =
            ChunkCode::try_for_language_and_chunk_size(SupportedLanguages::Rust, 32).unwrap();
        let source = TextNode::builder()
            .path("fixtures/日本語.rs")
            .chunk(code)
            .metadata(Metadata::from([("language", "Rust")]))
            .original_size(code.len())
            .offset(99usize)
            .build()
            .unwrap();
        let chunks = transformer.chunker.split(&source.chunk).unwrap();
        assert!(chunks.len() > 1);
        let outputs: Vec<TextNode> = transformer
            .transform_node(source.clone())
            .await
            .try_collect()
            .await
            .unwrap();

        let mut offset = 0;
        for (output, chunk) in outputs.iter().zip(&chunks) {
            let expected = TextNode::build_from_other(&source)
                .chunk(chunk.clone())
                .offset(offset)
                .build()
                .unwrap();
            assert_eq!(output, &expected);
            offset += chunk.len();
        }
        assert_eq!(outputs.len(), chunks.len());

        let parent = uuid::Uuid::new_v4();
        let mut descendant = source;
        descendant.parent_id = Some(parent);
        let child_outputs: Vec<TextNode> = transformer
            .transform_node(descendant)
            .await
            .try_collect()
            .await
            .unwrap();
        assert_eq!(child_outputs.len(), outputs.len());
        assert!(
            child_outputs
                .iter()
                .all(|output| output.parent_id == Some(parent))
        );
    }
}
