use crate::querying;

use super::{DEFAULT_TOP_K, SearchFilter};

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

/// The unfiltered strategy.
///
/// Only implemented for the unit filter so `SimilaritySingleEmbedding::default()` infers its
/// filter type, even for stores that also retrieve with a typed filter. Use
/// [`SimilaritySingleEmbedding::from_filter`] or [`SimilaritySingleEmbedding::with_filter`] for a
/// filtered search.
impl Default for SimilaritySingleEmbedding {
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
            top_k: DEFAULT_TOP_K,
            filter: Some(filter),
        }
    }

    /// Set the maximum amount of documents to be returned
    #[must_use]
    pub fn with_top_k(mut self, top_k: u64) -> Self {
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

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use async_trait::async_trait;

    use super::*;
    use crate::{
        Retrieve,
        querying::{Query, states},
    };

    /// Like `LanceDB` and `PgVector`, retrieves both unfiltered and with a `String` filter, and
    /// reports which one ran
    #[derive(Clone)]
    struct StringFilterStore;

    #[async_trait]
    impl Retrieve<SimilaritySingleEmbedding> for StringFilterStore {
        async fn retrieve(
            &self,
            search_strategy: &SimilaritySingleEmbedding,
            query: Query<states::Pending>,
        ) -> Result<Query<states::Retrieved>> {
            Ok(query.retrieved_documents(vec![
                format!("unfiltered top_k={}", search_strategy.top_k()).into(),
            ]))
        }
    }

    #[async_trait]
    impl Retrieve<SimilaritySingleEmbedding<String>> for StringFilterStore {
        async fn retrieve(
            &self,
            search_strategy: &SimilaritySingleEmbedding<String>,
            query: Query<states::Pending>,
        ) -> Result<Query<states::Retrieved>> {
            let filter = search_strategy.filter().clone().unwrap_or_default();
            Ok(query.retrieved_documents(vec![format!("filtered by {filter}").into()]))
        }
    }

    #[tokio::test]
    async fn default_infers_the_unfiltered_strategy_for_stores_with_typed_filters() {
        let result = StringFilterStore
            .retrieve(
                &SimilaritySingleEmbedding::default().with_top_k(3),
                Query::<states::Pending>::new("query"),
            )
            .await
            .unwrap();

        assert_eq!(result.documents()[0].content(), "unfiltered top_k=3");
    }

    #[tokio::test]
    async fn typed_filter_still_selects_the_filtered_retrieval() {
        let result = StringFilterStore
            .retrieve(
                &SimilaritySingleEmbedding::from_filter("a = 1".to_string()),
                Query::<states::Pending>::new("query"),
            )
            .await
            .unwrap();

        assert_eq!(result.documents()[0].content(), "filtered by a = 1");
    }

    #[test]
    fn default_has_no_filter_and_default_top_k() {
        let strategy = SimilaritySingleEmbedding::default();

        assert_eq!(strategy.top_k(), DEFAULT_TOP_K);
        assert!(strategy.filter().is_none());
    }

    #[test]
    fn with_top_k_chains_from_default() {
        let strategy = SimilaritySingleEmbedding::default().with_top_k(3);

        assert_eq!(strategy.top_k(), 3);
    }

    #[test]
    fn with_filter_keeps_top_k() {
        let strategy = SimilaritySingleEmbedding::default()
            .with_top_k(3)
            .with_filter("category = 'docs'".to_string());

        assert_eq!(strategy.top_k(), 3);
        assert_eq!(strategy.filter().as_deref(), Some("category = 'docs'"));
    }

    #[test]
    fn from_filter_uses_default_top_k() {
        let strategy = SimilaritySingleEmbedding::from_filter("a = 1".to_string()).with_top_k(4);

        assert_eq!(strategy.top_k(), 4);
        assert_eq!(strategy.filter().as_deref(), Some("a = 1"));
    }
}
