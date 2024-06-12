use anyhow::Result;
use async_trait::async_trait;
use futures_util::{stream, StreamExt};

use crate::{
    ingestion::{IngestionNode, IngestionStream},
    integrations::treesitter::{ChunkSize, CodeSplitter, SupportedLanguages},
    ChunkerTransformer,
};

#[derive(Debug)]
pub struct ChunkCode {
    chunker: CodeSplitter,
}

impl ChunkCode {
    pub fn for_language(lang: impl TryInto<SupportedLanguages>) -> Result<Self> {
        Ok(Self {
            chunker: CodeSplitter::builder()
                .try_language(lang)?
                .build()
                .expect("Failed to build code splitter"),
        })
    }

    pub fn for_language_and_chunk_size(
        lang: impl Into<SupportedLanguages>,
        chunk_size: impl Into<ChunkSize>,
    ) -> Result<Self> {
        Ok(Self {
            chunker: CodeSplitter::builder()
                .try_language(lang)?
                .chunk_size(chunk_size)
                .build()
                .expect("Failed to build code splitter"),
        })
    }
}

#[async_trait]
impl ChunkerTransformer for ChunkCode {
    #[tracing::instrument(skip_all, name = "transformers.chunk_code")]
    async fn transform_node(&self, node: IngestionNode) -> IngestionStream {
        let split_result = self.chunker.split(&node.chunk);

        if let Ok(split) = split_result {
            return stream::iter(split.into_iter().map(move |chunk| {
                Ok(IngestionNode {
                    chunk,
                    ..node.clone()
                })
            }))
            .boxed();
        } else {
            // Send the error downstream
            return stream::iter(vec![Err(split_result.unwrap_err())]).boxed();
        }
    }
}
