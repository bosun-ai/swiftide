pub type Embedding = Vec<f32>;
pub type Embeddings = Vec<Embedding>;

pub struct SparseEmbedding {
    pub indices: Vec<u32>,
    pub values: Vec<f32>,
}
pub type SparseEmbeddings = Vec<SparseEmbedding>;
