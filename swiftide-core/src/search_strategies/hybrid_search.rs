use derive_builder::Builder;

use crate::{indexing::EmbeddedField, querying};

use super::{DEFAULT_TOP_K, DEFAULT_TOP_N};

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

impl querying::SearchStrategy for HybridSearch {}

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
