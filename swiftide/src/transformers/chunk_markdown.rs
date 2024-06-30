use crate::{ingestion::IngestionNode, ingestion::IngestionStream, ChunkerTransformer};
use async_trait::async_trait;
use derive_builder::Builder;
use text_splitter::{Characters, MarkdownSplitter};

#[derive(Debug, Builder)]
#[builder(pattern = "owned", setter(strip_option))]
/// A transformer that chunks markdown content into smaller pieces.
///
/// The transformer will split the markdown content into smaller pieces based on the specified
/// `max_characters` or `range` of characters.
///
/// For further customization, you can use the builder to create a custom splitter.
///
/// Technically that might work with every splitter `text_splitter` provides.
pub struct ChunkMarkdown {
    chunker: MarkdownSplitter<Characters>,
    #[builder(default)]
    /// The number of concurrent chunks to process.
    concurrency: Option<usize>,
    /// The splitter is not perfect in skipping min size nodes.
    ///
    /// If you provide a custom chunker, you might want to set the range as well.
    #[builder(default)]
    range: Option<std::ops::Range<usize>>,
}

impl ChunkMarkdown {
    /// Create a new transformer with a maximum number of characters per chunk.
    pub fn from_max_characters(max_characters: usize) -> Self {
        Self {
            chunker: MarkdownSplitter::new(max_characters),
            concurrency: None,
            range: None,
        }
    }

    /// Create a new transformer with a range of characters per chunk.
    ///
    /// Chunks smaller than the range will be ignored.
    pub fn from_chunk_range(range: std::ops::Range<usize>) -> Self {
        Self {
            chunker: MarkdownSplitter::new(range.clone()),
            concurrency: None,
            range: Some(range),
        }
    }

    /// Build a custom markdown chunker.
    pub fn builder() -> ChunkMarkdownBuilder {
        ChunkMarkdownBuilder::default()
    }

    /// Set the number of concurrent chunks to process.
    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = Some(concurrency);
        self
    }

    fn min_size(&self) -> usize {
        self.range.as_ref().map(|r| r.start).unwrap_or(0)
    }
}

#[async_trait]
impl ChunkerTransformer for ChunkMarkdown {
    #[tracing::instrument(skip_all, name = "transformers.chunk_markdown")]
    async fn transform_node(&self, node: IngestionNode) -> IngestionStream {
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

        IngestionStream::iter(chunks.into_iter().map(move |chunk| {
            Ok(IngestionNode {
                chunk,
                ..node.clone()
            })
        }))
    }

    fn concurrency(&self) -> Option<usize> {
        self.concurrency
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use futures_util::stream::TryStreamExt;

    const MARKDOWN: &str = r#"
        # Hello, world!

        This is a test markdown document.

        ## Section 1

        This is a paragraph.

        ## Section 2

        This is another paragraph.
        "#;

    #[tokio::test]
    async fn test_transforming_with_max_characters_and_trimming() {
        let chunker = ChunkMarkdown::from_max_characters(40);

        let node = IngestionNode {
            chunk: MARKDOWN.to_string(),
            ..IngestionNode::default()
        };

        let nodes: Vec<IngestionNode> = chunker
            .transform_node(node)
            .await
            .try_collect()
            .await
            .unwrap();

        for line in MARKDOWN.lines().filter(|line| !line.trim().is_empty()) {
            assert!(nodes.iter().any(|node| node.chunk == line.trim()));
        }

        assert_eq!(nodes.len(), 6);
    }

    #[tokio::test]
    async fn test_always_within_range() {
        let ranges = vec![(10..15), (20..25), (30..35), (40..45), (50..55)];
        for range in ranges {
            let chunker = ChunkMarkdown::from_chunk_range(range.clone());
            let node = IngestionNode {
                chunk: MARKDOWN.to_string(),
                ..IngestionNode::default()
            };
            let nodes: Vec<IngestionNode> = chunker
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
