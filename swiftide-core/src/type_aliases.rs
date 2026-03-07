#![cfg_attr(coverage_nightly, coverage(off))]

use serde::{Deserialize, Serialize};

pub type Embedding = Vec<f32>;
pub type Embeddings = Vec<Embedding>;

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct SparseEmbedding {
    pub indices: Vec<u32>,
    pub values: Vec<f32>,
}
pub type SparseEmbeddings = Vec<SparseEmbedding>;

impl std::fmt::Debug for SparseEmbedding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SparseEmbedding")
            .field("indices", &self.indices.len())
            .field("values", &self.values.len())
            .finish()
    }
}
