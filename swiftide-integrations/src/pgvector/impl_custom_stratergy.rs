//! Custom search strategy implementation for `PostgreSQL` vector similarity search.
//!
//! Provides integration between the core search strategy framework and pgvector:
//! - Custom query generation for vector similarity searches
//! - Type-safe wrapper around `PostgreSQL` query builders
//! - Builder pattern for configuring search parameters
//!
//! The main type `PgVecCustomStrategy` implements the search strategy pattern
//! for `PostgreSQL` vector operations, allowing flexible and efficient similarity
//! searches with custom query generation capabilities.

use swiftide_core::querying::{self, search_strategies::CustomQuery};

/// `PostgreSQL`-specific implementation of `CustomQuery` for vector similarity search.
///
/// This type wraps `CustomQuery` with `PostgreSQL's` `QueryBuilder` to provide
/// type-safe query construction for pgvector operations.    
#[derive(Default, Clone)]
pub struct PgVecCustomStrategy(pub CustomQuery<sqlx::QueryBuilder<'static, sqlx::Postgres>>);

impl querying::SearchStrategy for PgVecCustomStrategy {}

impl std::ops::Deref for PgVecCustomStrategy {
    type Target = CustomQuery<sqlx::QueryBuilder<'static, sqlx::Postgres>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for PgVecCustomStrategy {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(test)]
mod tests {
    use super::PgVecCustomStrategy;
    use swiftide_core::querying::{search_strategies::CustomQuery, states, Query};
    #[tokio::test]
    async fn test_custom_query_builder_validation() {
        let strategy = PgVecCustomStrategy(CustomQuery::default().with_top_k(10));

        let query = Query::<states::Pending>::new("test_query");

        assert!(strategy.build_query(&query).is_err());
    }
}
