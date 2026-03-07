//! Search strategies provide a generic way for Retrievers to implement their
//! search in various ways.
//!
//! The strategy is also yielded to the Retriever and can contain addition configuration

mod custom_strategy;
mod hybrid_search;
mod similarity_single_embedding;

pub(crate) const DEFAULT_TOP_K: u64 = 10;
pub(crate) const DEFAULT_TOP_N: u64 = 10;

pub use custom_strategy::*;
pub use hybrid_search::*;
pub use similarity_single_embedding::*;

pub trait SearchFilter: Clone + Sync + Send {}

#[cfg(feature = "qdrant")]
impl SearchFilter for qdrant_client::qdrant::Filter {}

// When no filters are applied
impl SearchFilter for () {}
// Lancedb uses a string filter
impl SearchFilter for String {}
