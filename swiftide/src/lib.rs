//! The `swiftide` crate serves as the main entry point for the Swiftide project, providing a modular and organized structure for various functionalities.
//! This file declares and re-exports several modules to make core functionalities available throughout the project.

/// The `embeddings` module handles functionalities related to embeddings, which are essential for various machine learning and data processing tasks.
pub mod embeddings;

/// The `ingestion` module is responsible for the ingestion process, allowing the system to efficiently process and manage incoming data.
pub mod ingestion;

/// The `integrations` module facilitates integration with external systems, enabling seamless interaction with various services and platforms.
pub mod integrations;

/// The `loaders` module provides functionalities for loading data from different sources, ensuring data is readily available for processing.
pub mod loaders;

/// The `traits` module defines common traits used throughout the project, promoting code reuse and consistency.
pub mod traits;

/// The `transformers` module is responsible for transforming data, making it suitable for various processing tasks.
pub mod transformers;

/// Re-exporting the `embeddings` module to make its functionalities easily accessible throughout the project.
pub use embeddings::*;

/// Re-exporting the `traits` module to make common traits available throughout the project.
pub use traits::*;
