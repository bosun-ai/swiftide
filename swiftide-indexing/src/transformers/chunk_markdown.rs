//! Chunk markdown content into smaller pieces
use std::sync::Arc;

use async_trait::async_trait;
use derive_builder::Builder;
use swiftide_core::{indexing::IndexingStream, indexing::Node, ChunkerTransformer};
use text_splitter::{Characters, ChunkConfig, MarkdownSplitter};

const DEFAULT_MAX_CHAR_SIZE: usize = 2056;

#[derive(Clone, Builder)]
#[builder(setter(strip_option))]
/// A transformer that chunks markdown content into smaller pieces.
///
/// The transformer will split the markdown content into smaller pieces based on the specified
/// `max_characters` or `range` of characters.
///
/// For further customization, you can use the builder to create a custom splitter.
///
/// Technically that might work with every splitter `text_splitter` provides.
pub struct ChunkMarkdown {
    /// Defaults to `None`. If you use a splitter that is resource heavy, this parameter can be
    /// tuned.
    #[builder(default)]
    concurrency: Option<usize>,

    /// Optional maximum number of characters per chunk.
    ///
    /// Defaults to [`DEFAULT_MAX_CHAR_SIZE`].
    #[builder(default = "DEFAULT_MAX_CHAR_SIZE")]
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

    /// The markdown splitter from [`text_splitter`]
    ///
    /// Defaults to a new [`MarkdownSplitter`] with the specified `max_characters`.
    #[builder(setter(into), default = "self.default_client()")]
    chunker: Arc<MarkdownSplitter<Characters>>,
}

impl std::fmt::Debug for ChunkMarkdown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChunkMarkdown")
            .field("concurrency", &self.concurrency)
            .field("max_characters", &self.max_characters)
            .field("range", &self.range)
            .finish()
    }
}

impl Default for ChunkMarkdown {
    fn default() -> Self {
        Self::from_max_characters(DEFAULT_MAX_CHAR_SIZE)
    }
}

impl ChunkMarkdown {
    pub fn builder() -> ChunkMarkdownBuilder {
        ChunkMarkdownBuilder::default()
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

    fn min_size(&self) -> usize {
        self.range.start
    }
}

impl ChunkMarkdownBuilder {
    fn default_client(&self) -> Arc<MarkdownSplitter<Characters>> {
        let chunk_config: ChunkConfig<Characters> = self
            .range
            .clone()
            .map(ChunkConfig::<Characters>::from)
            .or_else(|| self.max_characters.map(Into::into))
            .unwrap_or(DEFAULT_MAX_CHAR_SIZE.into());

        Arc::new(MarkdownSplitter::new(chunk_config))
    }
}

#[async_trait]
impl ChunkerTransformer for ChunkMarkdown {
    #[tracing::instrument(skip_all)]
    async fn transform_node(&self, node: Node) -> IndexingStream {
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

        IndexingStream::iter(
            chunks
                .into_iter()
                .map(move |chunk| Node::build_from_other(&node).chunk(chunk).build()),
        )
    }

    fn concurrency(&self) -> Option<usize> {
        self.concurrency
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use futures_util::stream::TryStreamExt;

    const MARKDOWN: &str = r"
        # Hello, world!

        This is a test markdown document.

        ## Section 1

        This is a paragraph.

        ## Section 2

        This is another paragraph.
        ";

    #[tokio::test]
    async fn test_transforming_with_max_characters_and_trimming() {
        let chunker = ChunkMarkdown::from_max_characters(40);

        let node = Node::new(MARKDOWN.to_string());

        let nodes: Vec<Node> = chunker
            .transform_node(node)
            .await
            .try_collect()
            .await
            .unwrap();

        dbg!(&nodes.iter().map(|n| n.chunk.clone()).collect::<Vec<_>>());
        for line in MARKDOWN.lines().filter(|line| !line.trim().is_empty()) {
            nodes
                .iter()
                .find(|node| node.chunk == line.trim())
                .unwrap_or_else(|| panic!("Line not found: {line}"));
        }

        assert_eq!(nodes.len(), 6);
    }

    #[tokio::test]
    async fn test_always_within_range() {
        let ranges = vec![(10..15), (20..25), (30..35), (40..45), (50..55)];
        for range in ranges {
            let chunker = ChunkMarkdown::from_chunk_range(range.clone());
            let node = Node::new(MARKDOWN.to_string());
            let nodes: Vec<Node> = chunker
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
    fn test_builder() {
        ChunkMarkdown::builder()
            .chunker(MarkdownSplitter::new(40))
            .concurrency(10)
            .range(10..20)
            .build()
            .unwrap();
    }
}
