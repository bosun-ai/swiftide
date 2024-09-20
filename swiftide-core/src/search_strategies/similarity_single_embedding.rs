use crate::querying;

use super::{SearchFilter, DEFAULT_TOP_K};

/// A simple, single vector similarity search where it takes the embedding on the current query
/// and returns `top_k` documents.
///
/// Can optionally be used with a filter.
#[derive(Debug, Clone)]
pub struct SimilaritySingleEmbedding<FILTER: SearchFilter = ()> {
    /// Maximum number of documents to return
    top_k: u64,

    filter: Option<FILTER>,
}

impl<FILTER: SearchFilter> querying::SearchStrategy for SimilaritySingleEmbedding<FILTER> {}

impl<FILTER: SearchFilter> Default for SimilaritySingleEmbedding<FILTER> {
    fn default() -> Self {
        Self {
            top_k: DEFAULT_TOP_K,
            filter: None,
        }
    }
}

impl SimilaritySingleEmbedding<()> {
    /// Set an optional filter to be used in the query
    pub fn into_concrete_filter<FILTER: SearchFilter>(&self) -> SimilaritySingleEmbedding<FILTER> {
        SimilaritySingleEmbedding::<FILTER> {
            top_k: self.top_k,
            filter: None,
        }
    }
}

impl<FILTER: SearchFilter> SimilaritySingleEmbedding<FILTER> {
    pub fn from_filter(filter: FILTER) -> Self {
        Self {
            filter: Some(filter),
            ..Default::default()
        }
    }

    /// Set the maximum amount of documents to be returned
    pub fn with_top_k(&mut self, top_k: u64) -> &mut Self {
        self.top_k = top_k;

        self
    }

    /// Returns the maximum of documents to be returned
    pub fn top_k(&self) -> u64 {
        self.top_k
    }

    /// Set an optional filter to be used in the query
    pub fn with_filter<NEWFILTER: SearchFilter>(
        self,
        filter: NEWFILTER,
    ) -> SimilaritySingleEmbedding<NEWFILTER> {
        SimilaritySingleEmbedding::<NEWFILTER> {
            top_k: self.top_k,
            filter: Some(filter),
        }
    }

    pub fn filter(&self) -> &Option<FILTER> {
        &self.filter
    }
}
