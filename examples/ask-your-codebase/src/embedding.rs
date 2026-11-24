//! The local embedding model shared by indexing and querying.
//!
//! Both sides must use the same model, otherwise query vectors and chunk vectors live in
//! different spaces. Changing the model means deleting the index and indexing again.

use anyhow::Result;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use swiftide::integrations::fastembed::FastEmbed;

/// Vector size of `jina-embeddings-v2-base-code`.
pub const DIMENSIONS: i32 = 768;
/// Number of chunks embedded per call.
pub const BATCH_SIZE: usize = 32;
/// Tokens per embedded text. A full chunk plus its generated Q&A fits comfortably.
const MAX_TOKENS: usize = 2048;

// ANCHOR: embedding
/// A code-aware embedding model that runs on the CPU and needs no API key.
///
/// The model (~600MB) is downloaded on first use.
pub fn local_model() -> Result<FastEmbed> {
    let model = TextEmbedding::try_new(
        InitOptions::new(EmbeddingModel::JinaEmbeddingsV2BaseCode).with_max_length(MAX_TOKENS),
    )?;

    FastEmbed::builder()
        .embedding_model(model)
        .batch_size(BATCH_SIZE)
        .build()
}
// ANCHOR_END: embedding
