//! This module defines the `IngestionStream` type, which is used for handling asynchronous streams of `IngestionNode` items in the ingestion pipeline.
//!
//! The `IngestionStream` type is a pinned, boxed, dynamically-dispatched stream that yields `Result<IngestionNode>` items. This type is essential for managing
//! and processing large volumes of data asynchronously, ensuring efficient and scalable ingestion workflows.

use anyhow::Result;
use futures_util::stream::Stream;
use std::pin::Pin;

use super::IngestionNode;

/// A type alias for a pinned, boxed, dynamically-dispatched stream of `IngestionNode` items.
///
/// This type is used in the ingestion pipeline to handle asynchronous streams of data. Each item in the stream is a `Result<IngestionNode>`,
/// allowing for error handling during the ingestion process. The `Send` trait is implemented to ensure that the stream can be safely sent
/// across threads, enabling concurrent processing.
///
/// # Type Definition
/// - `Pin<Box<dyn Stream<Item = Result<IngestionNode>> + Send>>`
///
/// # Components
/// - `Pin`: Ensures that the memory location of the stream is fixed, which is necessary for certain asynchronous operations.
/// - `Box<dyn Stream<Item = Result<IngestionNode>>>`: A heap-allocated, dynamically-dispatched stream that yields `Result<IngestionNode>` items.
/// - `Send`: Ensures that the stream can be sent across thread boundaries, facilitating concurrent processing.
///
/// # Usage
/// The `IngestionStream` type is typically used in the ingestion pipeline to process data asynchronously. It allows for efficient handling
/// of large volumes of data by leveraging Rust's asynchronous capabilities.
///
/// # Error Handling
/// Each item in the stream is a `Result<IngestionNode>`, which means that errors can be propagated and handled during the ingestion process.
/// This design allows for robust error handling and ensures that the ingestion pipeline can gracefully handle failures.
///
/// # Performance Considerations
/// The use of `Pin` and `Box` ensures that the stream's memory location is fixed and heap-allocated, respectively. This design choice is
/// crucial for asynchronous operations that require stable memory addresses. Additionally, the `Send` trait enables concurrent processing,
/// which can significantly improve performance in multi-threaded environments.
///
/// # Edge Cases
/// - The stream may yield errors (`Err` variants) instead of valid `IngestionNode` items. These errors should be handled appropriately
///   to ensure the robustness of the ingestion pipeline.
/// - The stream must be pinned to ensure that its memory location remains fixed, which is necessary for certain asynchronous operations.

pub type IngestionStream = Pin<Box<dyn Stream<Item = Result<IngestionNode>> + Send>>;
