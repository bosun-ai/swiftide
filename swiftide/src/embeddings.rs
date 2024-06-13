/// This module defines a type alias for handling embedding vectors within the Swiftide project.
///
/// The `Embeddings` type alias is used throughout the project to represent a collection of embedding vectors,
/// which are commonly used in machine learning and natural language processing tasks.
///
/// # Usage
///
/// The `Embeddings` type alias is utilized in various parts of the Swiftide project, including the OpenAI integration module.
/// For example, in the `embed.rs` file, the `Embeddings` type alias is used to represent the result of an embedding operation.

pub type Embeddings = Vec<Vec<f32>>;
