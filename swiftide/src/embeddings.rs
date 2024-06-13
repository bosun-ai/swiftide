/// This module defines a type alias for handling embedding vectors within the Swiftide project.
///
/// The `Embeddings` type alias is used throughout the project to represent a collection of embedding vectors,
/// which are commonly used in machine learning and natural language processing tasks.
///
/// # Type Alias
///
/// `Embeddings` is a type alias for `Vec<Vec<f32>>`, which is a vector of vectors of 32-bit floating-point numbers.
/// This structure is chosen to efficiently handle and store embedding vectors.
///
/// # Usage
///
/// The `Embeddings` type alias is utilized in various parts of the Swiftide project, including the OpenAI integration module.
/// For example, in the `embed.rs` file, the `Embeddings` type alias is used to represent the result of an embedding operation.
///
/// ```rust
/// use crate::{Embed, Embeddings};
///
/// #[async_trait]
/// impl Embed for OpenAI {
///     async fn embed(&self, input: Vec<String>) -> Result<Embeddings> {
///         // Embedding logic
///         Ok(response.data.into_iter().map(|d| d.embedding).collect())
///     }
/// }
/// ```
///
/// This type alias improves code readability and maintainability by providing a clear and concise way to refer to embedding vectors.

pub type Embeddings = Vec<Vec<f32>>;
