//! Swiftide - Document and code indexation for retrieval augmented generation
//!
//! Swiftide is a straightforward, easy-to-use, easy-to-extend asynchronous file ingestion and processing system. It is designed to be used in a RAG (Retrieval Augmented Generation) system. It is built to be fast and efficient, with a focus on parallel processing and asynchronous operations.
//!
//! Part of the bosun.ai project. An upcoming platform for autonomous code improvement.
//!
//! We <3 feedback: project ideas, suggestions, and complaints are very welcome. Feel free to open an issue.
//!
//! # Feature flags
//! Swiftide has little features enabled by default as there are some dependency heavy
//! integrations.
//!
//! Either use the 'all' feature flag (not recommended), or enable the integrations that you need.
//! Each integration has a similarly named feature flag.

pub mod indexing;
pub mod integrations;
pub mod loaders;
pub mod persist;
pub mod traits;
pub mod transformers;
pub mod type_aliases;

pub use traits::*;
pub use type_aliases::*;

/// Deprecated re-export of `indexing`, use that instead.
#[deprecated(
    since = "0.6.0",
    note = "Renamed references of Ingestion to Indexing for more appropriate naming. Will be removed in a future release."
)]
pub mod ingestion {
    pub use crate::indexing::*;

    pub use crate::indexing::IndexingStream as IngestionStream;
    pub use crate::indexing::Node as IngestionNode;
    pub use crate::indexing::Pipeline as IngestionPipeline;
}
