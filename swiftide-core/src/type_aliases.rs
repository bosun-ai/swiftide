use serde::{Deserialize, Serialize};

pub type Embedding = Vec<f32>;
pub type Embeddings = Vec<Embedding>;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SparseEmbedding {
    pub indices: Vec<usize>,
    pub values: Vec<f32>,
}
pub type SparseEmbeddings = Vec<SparseEmbedding>;
