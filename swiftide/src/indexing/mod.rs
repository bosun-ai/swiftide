//! This module serves as the main entry point for the ingestion components in the Swiftide project.
//! It re-exports the essential structs and types from the `ingestion_node`, `ingestion_pipeline`,
//! and `ingestion_stream` modules, providing a unified interface for the ingestion functionality.
//!
//! The ingestion system in Swiftide is designed to handle the asynchronous processing of large volumes
//! of data, including loading, transforming, and storing data chunks. The primary components include:
//!
//! - `IngestionNode`: Represents a unit of data in the ingestion process, encapsulating metadata, data chunks,
//!   and optional vector representations.
//! - `IngestionPipeline`: Orchestrates the entire file ingestion process, allowing for various stages of data
//!   transformation and storage to be configured and executed asynchronously.
//! - `IngestionStream`: A type alias for a pinned, boxed, dynamically-dispatched stream of `IngestionNode` items,
//!   facilitating efficient and scalable ingestion workflows.
//!
//! # Usage
//!
//! The components re-exported by this module are used throughout the Swiftide project to build and manage
//! ingestion pipelines. These pipelines can be customized with different loaders, transformers, and storage
//! backends to meet specific requirements.

mod indexing_stream;
mod node;
mod pipeline;

pub use indexing_stream::*;
pub use node::*;
pub use pipeline::*;
