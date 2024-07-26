use crate::querying;

#[derive(Debug, Default, Clone, Copy)]
pub struct SimilaritySingleEmbedding {}

impl querying::SearchStrategy for SimilaritySingleEmbedding {}
