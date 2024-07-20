use crate::traits;

#[derive(Debug, Default, Clone, Copy)]
pub struct SimilaritySingleEmbedding {}

impl traits::SearchStrategyMarker for SimilaritySingleEmbedding {}
