//! This module serves as the main entry point for the indexing components in the Swiftide project.
//! It re-exports the essential structs and types from the `indexing_node`, `indexing_pipeline`,
//! and `indexing_stream` modules, providing a unified interface for the indexing functionality.
//!
//! The indexing system in Swiftide is designed to handle the asynchronous processing of large volumes
//! of data, including loading, transforming, and storing data chunks. The primary components include:
//!
//! - `Node`: Represents a unit of data in the indexing process, encapsulating metadata, data chunks,
//!   and optional vector representations.
//! - `Pipeline`: Orchestrates the entire file indexing process, allowing for various stages of data
//!   transformation and storage to be configured and executed asynchronously.
//! - `IndexingStream`: A type alias for a pinned, boxed, dynamically-dispatched stream of `Node` items,
//!   facilitating efficient and scalable indexing workflows.
//!
//! # Usage
//!
//! The components re-exported by this module are used throughout the Swiftide project to build and manage
//! indexing pipelines. These pipelines can be customized with different loaders, transformers, and storage
//! backends to meet specific requirements.

mod indexing_stream;
mod node;
mod pipeline;

pub use indexing_stream::*;
pub use node::*;
pub use pipeline::*;
