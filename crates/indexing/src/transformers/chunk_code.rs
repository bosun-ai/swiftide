use crate::{
    ingestion_node::IngestionNode, ingestion_pipeline::IngestionStream, traits::ChunkerTransformer,
};
use async_trait::async_trait;
use code_ops::{ChunkSize, CodeSplitter};
use futures_util::{stream, StreamExt};
use infrastructure::SupportedLanguages;

#[derive(Debug)]
pub struct ChunkCode {
    chunker: CodeSplitter,
}

impl ChunkCode {
    pub fn for_language(lang: impl Into<SupportedLanguages>) -> Self {
        let lang = lang.into();
        Self {
            chunker: CodeSplitter::builder()
                .language(lang)
                .build()
                .expect("Failed to build code splitter"),
        }
    }

    pub fn for_language_and_chunk_size(
        lang: impl Into<SupportedLanguages>,
        chunk_size: impl Into<ChunkSize>,
    ) -> Self {
        let lang = lang.into();
        Self {
            chunker: CodeSplitter::builder()
                .language(lang)
                .chunk_size(chunk_size.into())
                .build()
                .expect("Failed to build code splitter"),
        }
    }
}

#[async_trait]
impl ChunkerTransformer for ChunkCode {
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
