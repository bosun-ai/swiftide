use crate::{ingestion::IngestionNode, ingestion::IngestionStream, ChunkerTransformer};
use async_trait::async_trait;
use derive_builder::Builder;
use futures_util::{stream, StreamExt};
use text_splitter::{Characters, MarkdownSplitter};

#[derive(Debug, Builder)]
#[builder(pattern = "owned")]
pub struct ChunkMarkdown {
    chunker: MarkdownSplitter<Characters>,
    #[builder(default)]
    concurrency: Option<usize>,
}

impl ChunkMarkdown {
    pub fn with_max_characters(max_characters: usize) -> Self {
        Self {
            chunker: MarkdownSplitter::new(max_characters),
            concurrency: None,
        }
    }

    pub fn with_chunk_range(range: std::ops::Range<usize>) -> Self {
        Self {
            chunker: MarkdownSplitter::new(range),
            concurrency: None,
        }
    }

    pub fn builder() -> ChunkMarkdownBuilder {
        ChunkMarkdownBuilder::default()
    }
}

#[async_trait]
impl ChunkerTransformer for ChunkMarkdown {
    #[tracing::instrument(skip_all, name = "transformers.chunk_markdown")]
    async fn transform_node(&self, node: IngestionNode) -> IngestionStream {
        let chunks = self
            .chunker
            .chunks(&node.chunk)
            .map(|chunk| chunk.to_string())
            .collect::<Vec<String>>();

        stream::iter(chunks.into_iter().map(move |chunk| {
            Ok(IngestionNode {
                chunk,
                ..node.clone()
            })
        }))
        .boxed()
    }

    fn concurrency(&self) -> Option<usize> {
        self.concurrency
    }
}
