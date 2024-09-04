use std::sync::Arc;

use derive_builder::Builder;

const DEFAULT_TOP_K: u64 = 10;
const DEFAULT_TOP_N: u64 = 10;

/// Search strategies provide a generic way for Retrievers to implement their
/// search in various ways.
///
/// The strategy is also yielded to the Retriever and can contain addition configuration
use crate::{indexing::EmbeddedField, querying};

/// A strategy that can be build with a generic query for the retriever to use
///
/// The retriever will manage extracting the documents, only the query is needed.
#[derive(Debug, Clone)]
pub struct CustomQuery<Q> {
    query: Arc<Q>,
}

impl<Q: Send + Sync + Clone> CustomQuery<Q> {
    pub fn from_query(query: Q) -> Self {
        CustomQuery {
            query: query.into(),
        }
    }

    pub fn query(&self) -> &Q {
        &self.query
    }
}

/// A very simple search where it takes the embedding on the current query
/// and returns `top_k` documents.
#[derive(Debug, Clone, Copy)]
pub struct SimilaritySingleEmbedding {
    /// Maximum number of documents to return
    top_k: u64,
}

/// A hybrid search strategy that combines a similarity search with a
/// keyword search / sparse search.
///
/// Defaults to a a maximum of 10 documents and `EmbeddedField::Combined` for the field(s).
#[derive(Debug, Clone, Builder)]
#[builder(setter(into))]
pub struct HybridSearch {
    /// Maximum number of documents to return
    #[builder(default)]
    top_k: u64,
    /// Maximum number of documents to return per query
    #[builder(default)]
    top_n: u64,

    /// The field to use for the dense vector
    #[builder(default)]
    dense_vector_field: EmbeddedField,

    /// The field to use for the sparse vector
    /// TODO: I.e. lancedb does not use sparse embeddings for hybrid search
    #[builder(default)]
    sparse_vector_field: EmbeddedField,
}

impl Default for HybridSearch {
    fn default() -> Self {
        Self {
            top_k: DEFAULT_TOP_K,
            top_n: DEFAULT_TOP_N,
            dense_vector_field: EmbeddedField::Combined,
            sparse_vector_field: EmbeddedField::Combined,
        }
    }
}

impl HybridSearch {
    /// Sets the maximum amount of total documents retrieved
    pub fn with_top_k(&mut self, top_k: u64) -> &mut Self {
        self.top_k = top_k;
        self
    }
    /// Returns the maximum amount of total documents to be retrieved
    pub fn top_k(&self) -> u64 {
        self.top_k
    }
    /// Sets the maximum amount of documents to be retrieved
    /// per individual query
    pub fn with_top_n(&mut self, top_n: u64) -> &mut Self {
        self.top_n = top_n;
        self
    }
    /// Returns the maximum amount of documents per query
    pub fn top_n(&self) -> u64 {
        self.top_n
    }
    /// Sets the vector field for the dense vector
    ///
    /// Defaults to `EmbeddedField::Combined`
    pub fn with_dense_vector_field(
        &mut self,
        dense_vector_field: impl Into<EmbeddedField>,
    ) -> &mut Self {
        self.dense_vector_field = dense_vector_field.into();
        self
    }

    /// Returns the field for the dense vector
    pub fn dense_vector_field(&self) -> &EmbeddedField {
        &self.dense_vector_field
    }
    /// Sets the vector field for the sparse vector (if applicable)
    ///
    /// Defaults to `EmbeddedField::Combined`
    pub fn with_sparse_vector_field(
        &mut self,
        sparse_vector_field: impl Into<EmbeddedField>,
    ) -> &mut Self {
        self.sparse_vector_field = sparse_vector_field.into();
        self
    }

    /// Returns the field for the dense vector
    pub fn sparse_vector_field(&self) -> &EmbeddedField {
        &self.sparse_vector_field
    }
}

impl Default for SimilaritySingleEmbedding {
    fn default() -> Self {
        Self {
            top_k: DEFAULT_TOP_K,
        }
    }
}

impl SimilaritySingleEmbedding {
    /// Set the maximum amount of documents to be returned
    pub fn with_top_k(&mut self, top_k: u64) -> &mut Self {
        self.top_k = top_k;

        self
    }

    /// Returns the maximum of documents to be returned
    pub fn top_k(&self) -> u64 {
        self.top_k
    }
}

impl querying::SearchStrategy for SimilaritySingleEmbedding {}
impl querying::SearchStrategy for HybridSearch {}
impl<Q: Send + Sync + Clone> querying::SearchStrategy for CustomQuery<Q> {}
