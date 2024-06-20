use anyhow::Result;
use async_trait::async_trait;
use fastembed::TextEmbedding;

use crate::{Embed, Embeddings};

///! An integration for the embedding transformer for the FastEmbed library.
///!
///! See the [FastEmbed documentation](https://docs.rs/fastembed) for more information on usage.
///!
///! Requires the `fastembed` feature to be enabled.
///!
///! Models can be added directly to the Embed transformer.
///
#[async_trait]
impl Embed for TextEmbedding {
    async fn embed(&self, input: Vec<String>) -> Result<Embeddings> {
        self.embed(input, None)
    }
}
