use anyhow::Result;
use async_trait::async_trait;
use fastembed::TextEmbedding;

use crate::{EmbeddingModel, Embeddings};

///! An integration for the embedding transformer for the FastEmbed library.
///!
///! Supports a variety of fast text embedding models.
///!
///! See the [FastEmbed documentation](https://docs.rs/fastembed) for more information on usage.
///!
///! Requires the `fastembed` feature to be enabled.
///!
///! Models can be added directly to the Embed transformer.
///
#[async_trait]
impl EmbeddingModel for TextEmbedding {
    async fn embed(&self, input: Vec<String>) -> Result<Embeddings> {
        // NOTE: Opportunity to batch here
        self.embed(input, None)
    }
}
