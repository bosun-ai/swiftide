use std::thread::Builder;

use derive_builder::Builder;

/// Search strategies provide a generic way for Retrievers to implement their
/// search in various ways.
///
/// The strategy is also yielded to the Retriever and can contain addition configuration
use crate::{indexing::EmbeddedField, querying};

/// A very simple search where it takes the embedding on the current query
/// and returns `top_k` documents.
#[derive(Debug, Clone, Copy)]
pub struct SimilaritySingleEmbedding {
    /// Maximum number of documents to return
    top_k: u64,
}

/// A hybrid search strategy that combines a similarity search with a
/// keyword search.
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
            top_k: 10,
            top_n: 10,
            dense_vector_field: EmbeddedField::Combined,
            sparse_vector_field: EmbeddedField::Combined,
        }
    }
}

impl HybridSearch {
    pub fn with_top_k(&mut self, top_k: u64) -> &mut Self {
        self.top_k = top_k;
        self
    }
    pub fn top_k(&self) -> u64 {
        self.top_k
    }
    pub fn with_top_n(&mut self, top_n: u64) -> &mut Self {
        self.top_n = top_n;
        self
    }
    pub fn top_n(&self) -> u64 {
        self.top_n
    }
    pub fn with_dense_vector_field(
        &mut self,
        dense_vector_field: impl Into<EmbeddedField>,
    ) -> &mut Self {
        self.dense_vector_field = dense_vector_field.into();
        self
    }
    pub fn dense_vector_field(&self) -> &EmbeddedField {
        &self.dense_vector_field
    }
    pub fn with_sparse_vector_field(
        &mut self,
        sparse_vector_field: impl Into<EmbeddedField>,
    ) -> &mut Self {
        self.sparse_vector_field = sparse_vector_field.into();
        self
    }
    pub fn sparse_vector_field(&self) -> &EmbeddedField {
        &self.sparse_vector_field
    }
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
impl querying::SearchStrategy for HybridSearch {}
