use crate::traits;

#[derive(Debug, Default, Clone, Copy)]
pub enum SearchStrategy {
    #[default]
    SimilaritySingleEmbedding,
}

impl traits::SearchStrategy for SearchStrategy {}
