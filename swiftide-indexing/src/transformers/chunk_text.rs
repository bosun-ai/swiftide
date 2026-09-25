//! Chunk text content into smaller pieces
use std::sync::Arc;

use async_trait::async_trait;
use derive_builder::Builder;
use swiftide_core::{ChunkerTransformer, indexing::IndexingStream, indexing::TextNode};
use text_splitter::{Characters, ChunkConfig, TextSplitter};

const DEFAULT_MAX_CHAR_SIZE: usize = 2056;

#[derive(Debug, Clone, Builder)]
#[builder(setter(strip_option))]
/// A transformer that chunks text content into smaller pieces.
///
/// The transformer will split the text content into smaller pieces based on the specified
/// `max_characters` or `range` of characters.
///
/// For further customization, you can use the builder to create a custom splitter. Uses
/// `text_splitter` under the hood.
///
/// Technically that might work with every splitter `text_splitter` provides.
pub struct ChunkText {
    /// The max number of concurrent chunks to process.
    ///
    /// Defaults to `None`. If you use a splitter that is resource heavy, this parameter can be
    /// tuned.
    #[builder(default)]
    concurrency: Option<usize>,

    /// Optional maximum number of characters per chunk.
    ///
    /// Defaults to [`DEFAULT_MAX_CHAR_SIZE`].
    #[builder(default = "DEFAULT_MAX_CHAR_SIZE")]
    #[allow(dead_code)]
    max_characters: usize,

    /// A range of minimum and maximum characters per chunk.
    ///
    /// Chunks smaller than the range min will be ignored. `max_characters` will be ignored if this
    /// is set.
    ///
    /// If you provide a custom chunker with a range, you might want to set the range as well.
    ///
    /// Defaults to 0..[`max_characters`]
    #[builder(default = "0..DEFAULT_MAX_CHAR_SIZE")]
    range: std::ops::Range<usize>,

    /// The text splitter from [`text_splitter`]
    ///
    /// Defaults to a new [`TextSplitter`] with the specified `max_characters`.
    #[builder(setter(into), default = "self.default_client()")]
    chunker: Arc<TextSplitter<Characters>>,
}

impl Default for ChunkText {
    /// Creates a transformer using the default maximum chunk size.
    fn default() -> Self {
        Self::from_max_characters(DEFAULT_MAX_CHAR_SIZE)
    }
}

impl ChunkText {
    /// Creates a builder for configuring a text chunk transformer.
    pub fn builder() -> ChunkTextBuilder {
        ChunkTextBuilder::default()
    }

    /// Create a new transformer with a maximum number of characters per chunk.
    #[allow(clippy::missing_panics_doc)]
    pub fn from_max_characters(max_characters: usize) -> Self {
        Self::builder()
            .max_characters(max_characters)
            .build()
            .expect("Cannot fail")
    }

    /// Create a new transformer with a range of characters per chunk.
    ///
    /// Chunks smaller than the range will be ignored.
    #[allow(clippy::missing_panics_doc)]
    pub fn from_chunk_range(range: std::ops::Range<usize>) -> Self {
        Self::builder().range(range).build().expect("Cannot fail")
    }

    /// Set the number of concurrent chunks to process.
    #[must_use]
    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = Some(concurrency);
        self
    }

    /// Returns the minimum byte length accepted for a produced chunk.
    ///
    /// The configured range is expressed as characters, but filtering uses the chunk string's
    /// byte length, matching the existing transformer behavior.
    fn min_size(&self) -> usize {
        self.range.start
    }
}

impl ChunkTextBuilder {
    /// Builds the underlying splitter from the configured range or maximum size.
    fn default_client(&self) -> Arc<TextSplitter<Characters>> {
        let chunk_config: ChunkConfig<Characters> = self
            .range
            .clone()
            .map(ChunkConfig::<Characters>::from)
            .or_else(|| self.max_characters.map(Into::into))
            .unwrap_or(DEFAULT_MAX_CHAR_SIZE.into());

        Arc::new(TextSplitter::new(chunk_config))
    }
}
#[async_trait]
impl ChunkerTransformer for ChunkText {
    type Input = String;
    type Output = String;

    /// Splits text while preserving node fields and the original parent identity on every output.
    #[tracing::instrument(skip_all, name = "transformers.chunk_text")]
    async fn transform_node(&self, node: TextNode) -> IndexingStream<String> {
        let chunks = self
            .chunker
            .chunks(&node.chunk)
            .filter_map(|chunk| {
                let trim = chunk.trim();
                if trim.is_empty() || trim.len() < self.min_size() {
                    None
                } else {
                    Some(chunk.to_string())
                }
            })
            .collect::<Vec<String>>();
        IndexingStream::iter(node.into_chunks(chunks).map(Ok))
    }

    /// Returns the configured concurrency limit for chunk processing.
    fn concurrency(&self) -> Option<usize> {
        self.concurrency
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use futures_util::stream::TryStreamExt;

    const TEXT: &str = r"
        This is a text.

        This is a paragraph.

        This is another paragraph.
        ";

    #[tokio::test]
    /// Verifies maximum-size chunking trims boundaries and emits the expected paragraphs.
    async fn test_transforming_with_max_characters_and_trimming() {
        let chunker = ChunkText::from_max_characters(40);

        let node = TextNode::new(TEXT.to_string());

        let nodes: Vec<TextNode> = chunker
            .transform_node(node)
            .await
            .try_collect()
            .await
            .unwrap();

        for line in TEXT.lines().filter(|line| !line.trim().is_empty()) {
            assert!(nodes.iter().any(|node| node.chunk == line.trim()));
        }

        assert_eq!(nodes.len(), 3);
    }

    #[tokio::test]
    /// Verifies emitted chunks stay within each configured character range.
    async fn test_always_within_range() {
        let ranges = vec![(10..15), (20..25), (30..35), (40..45), (50..55)];
        for range in ranges {
            let chunker = ChunkText::from_chunk_range(range.clone());
            let node = TextNode::new(TEXT.to_string());
            let nodes: Vec<TextNode> = chunker
                .transform_node(node)
                .await
                .try_collect()
                .await
                .unwrap();
            // Assert all nodes chunk length within the range
            assert!(
                nodes.iter().all(|node| {
                    let len = node.chunk.len();
                    range.contains(&len)
                }),
                "{:?}, {:?}",
                range,
                nodes.iter().filter(|node| {
                    let len = node.chunk.len();
                    !range.contains(&len)
                })
            );
        }
    }

    #[test]
    /// Verifies the builder accepts a custom splitter, concurrency, and range.
    fn test_builder() {
        ChunkText::builder()
            .chunker(text_splitter::TextSplitter::new(40))
            .concurrency(10)
            .range(10..20)
            .build()
            .unwrap();
    }

    mod regression {
        use std::collections::HashMap;

        use futures_util::stream::TryStreamExt;
        use swiftide_core::ChunkerTransformer;
        use swiftide_core::SparseEmbedding;
        use swiftide_core::indexing::{EmbedMode, EmbeddedField, Metadata, TextNode};

        use super::ChunkText;

        /// Builds metadata used to verify field preservation.
        fn metadata() -> Metadata {
            Metadata::from([("language", "日本語"), ("kind", "regression")])
        }

        /// Builds dense vectors used to verify field preservation.
        fn dense_vectors() -> HashMap<EmbeddedField, Vec<f32>> {
            HashMap::from([
                (EmbeddedField::Chunk, vec![1.0, 2.0, 3.0]),
                (EmbeddedField::Metadata("kind".into()), vec![4.0, 5.0]),
            ])
        }

        /// Builds sparse vectors used to verify field preservation.
        fn sparse_vectors() -> HashMap<EmbeddedField, SparseEmbedding> {
            HashMap::from([(
                EmbeddedField::Chunk,
                SparseEmbedding {
                    indices: vec![2, 9],
                    values: vec![0.25, 0.75],
                },
            )])
        }

        /// Asserts that chunking preserves node fields and assigns the expected parent.
        fn assert_preserved(node: &TextNode, output: &TextNode, parent: Option<uuid::Uuid>) {
            assert_eq!(output.path, node.path);
            assert_eq!(output.metadata, node.metadata);
            assert_eq!(output.vectors, node.vectors);
            assert_eq!(output.sparse_vectors, node.sparse_vectors);
            assert_eq!(output.embed_mode, node.embed_mode);
            assert_eq!(output.original_size, node.original_size);
            assert_eq!(output.offset, node.offset);
            assert_eq!(output.parent_id, parent);
            assert!(!output.chunk.is_empty());
        }

        #[tokio::test]
        /// Verifies full field preservation and parent assignment when the input has no parent.
        async fn preserves_full_node_fields_and_assigns_absent_parent() {
            let source = "αβγ 日本語 metadata-preservation payload ".repeat(8);
            let node = TextNode::builder()
                .path("fixtures/unicode.txt")
                .chunk(source)
                .metadata(metadata())
                .vectors(dense_vectors())
                .sparse_vectors(sparse_vectors())
                .embed_mode(EmbedMode::Both)
                .original_size(4096usize)
                .offset(37usize)
                .build()
                .unwrap();
            let original_id = node.id();

            let outputs: Vec<TextNode> = ChunkText::from_chunk_range(1..32)
                .transform_node(node.clone())
                .await
                .try_collect()
                .await
                .unwrap();

            assert!(outputs.len() > 1);
            for output in &outputs {
                assert_preserved(&node, output, Some(original_id));
            }
        }

        #[tokio::test]
        /// Verifies existing parents, Unicode chunks, and blank-chunk filtering.
        async fn preserves_existing_parent_unicode_and_discards_blank_chunks() {
            let parent = uuid::Uuid::new_v4();
            let source = "\n\n  αβγ 日本語  \n\n café \n\t";
            let node = TextNode::builder()
                .path("fixtures/blank-and-unicode.txt")
                .chunk(source)
                .metadata(metadata())
                .vectors(dense_vectors())
                .sparse_vectors(sparse_vectors())
                .embed_mode(EmbedMode::Both)
                .original_size(source.len())
                .offset(11usize)
                .parent_id(parent)
                .build()
                .unwrap();

            let outputs: Vec<TextNode> = ChunkText::from_chunk_range(1..32)
                .transform_node(node.clone())
                .await
                .try_collect()
                .await
                .unwrap();

            assert!(!outputs.is_empty());
            assert!(outputs.iter().all(|output| !output.chunk.trim().is_empty()));
            assert!(outputs.iter().any(|output| output.chunk.contains("α")));
            assert!(outputs.iter().any(|output| output.chunk.contains("日本語")));
            assert!(outputs.iter().any(|output| output.chunk.contains("café")));
            for output in &outputs {
                assert_preserved(&node, output, Some(parent));
            }

            let blank = TextNode::new("\n \t\n");
            let blank_outputs: Vec<TextNode> = ChunkText::from_chunk_range(1..32)
                .transform_node(blank)
                .await
                .try_collect()
                .await
                .unwrap();
            assert!(blank_outputs.is_empty());
        }
    }
}
