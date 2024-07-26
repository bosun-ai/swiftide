
use crate::querying;

#[derive(Debug, Clone, Copy)]
pub struct SimilaritySingleEmbedding {
    top_k: u64,
}

impl Default for SimilaritySingleEmbedding {
    fn default() -> Self {
        Self { top_k: 10 }
    }
}

impl SimilaritySingleEmbedding {
    pub fn with_top_k(&mut self, top_k: u64) -> &mut Self {
        self.top_k = top_k;

        self
    }
    pub fn top_k(&self) -> u64 {
        self.top_k
    }
}

impl querying::SearchStrategy for SimilaritySingleEmbedding {}
