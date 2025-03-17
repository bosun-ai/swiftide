//! This module defines the `Node` struct and its associated methods.
//!
//! `Node` represents a unit of data in the indexing process, containing metadata,
//! the data chunk itself, and an optional vector representation.
//!
//! # Overview
//!
//! The `Node` struct is designed to encapsulate all necessary information for a single
//! unit of data being processed in the indexing pipeline. It includes fields for an identifier,
//! file path, data chunk, optional vector representation, and metadata.
//!
//! The struct provides methods to convert the node into an embeddable string format and to
//! calculate a hash value for the node based on its path and chunk.
//!
//! # Usage
//!
//! The `Node` struct is used throughout the indexing pipeline to represent and process
//! individual units of data. It is particularly useful in scenarios where metadata and data chunks
//! need to be processed together.
use std::{
    collections::HashMap,
    fmt::Debug,
    hash::{Hash, Hasher},
    os::unix::ffi::OsStrExt,
    path::PathBuf,
};

use derive_builder::Builder;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::{metadata::Metadata, util::debug_long_utf8, Embedding, SparseEmbedding};

/// Represents a unit of data in the indexing process.
///
/// `Node` encapsulates all necessary information for a single unit of data being processed
/// in the indexing pipeline. It includes fields for an identifier, file path, data chunk, optional
/// vector representation, and metadata.
#[derive(Default, Clone, Serialize, Deserialize, PartialEq, Builder)]
#[builder(setter(into, strip_option), build_fn(error = "anyhow::Error"))]
pub struct Node {
    /// File path associated with the node.
    #[builder(default)]
    pub path: PathBuf,
    /// Data chunk contained in the node.
    pub chunk: String,
    /// Optional vector representation of embedded data.
    #[builder(default)]
    pub vectors: Option<HashMap<EmbeddedField, Embedding>>,
    /// Optional sparse vector representation of embedded data.
    #[builder(default)]
    pub sparse_vectors: Option<HashMap<EmbeddedField, SparseEmbedding>>,
    /// Metadata associated with the node.
    #[builder(default)]
    pub metadata: Metadata,
    /// Mode of embedding data Chunk and Metadata
    #[builder(default)]
    pub embed_mode: EmbedMode,
    /// Size of the input this node was originally derived from in bytes
    #[builder(default)]
    pub original_size: usize,
    /// Offset of the chunk relative to the start of the input this node was originally derived
    /// from in bytes
    #[builder(default)]
    pub offset: usize,
}

impl NodeBuilder {
    pub fn maybe_sparse_vectors(
        &mut self,
        sparse_vectors: Option<HashMap<EmbeddedField, SparseEmbedding>>,
    ) -> &mut Self {
        self.sparse_vectors = Some(sparse_vectors);
        self
    }

    pub fn maybe_vectors(
        &mut self,
        vectors: Option<HashMap<EmbeddedField, Embedding>>,
    ) -> &mut Self {
        self.vectors = Some(vectors);
        self
    }
}

impl Debug for Node {
    /// Formats the node for debugging purposes.
    ///
    /// This method is used to provide a human-readable representation of the node when debugging.
    /// The vector field is displayed as the number of elements in the vector if present.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Node")
            .field("id", &self.id())
            .field("path", &self.path)
            .field("chunk", &debug_long_utf8(&self.chunk, 100))
            .field("metadata", &self.metadata)
            .field(
                "vectors",
                &self
                    .vectors
                    .iter()
                    .flat_map(HashMap::iter)
                    .map(|(embed_type, vec)| format!("'{embed_type}': {}", vec.len()))
                    .join(","),
            )
            .field(
                "sparse_vectors",
                &self
                    .sparse_vectors
                    .iter()
                    .flat_map(HashMap::iter)
                    .map(|(embed_type, vec)| {
                        format!(
                            "'{embed_type}': indices({}), values({})",
                            vec.indices.len(),
                            vec.values.len()
                        )
                    })
                    .join(","),
            )
            .field("embed_mode", &self.embed_mode)
            .finish()
    }
}

impl Node {
    /// Builds a new instance of `Node`, returning a `NodeBuilder`. Copies
    /// over the fields from the provided `Node`.
    pub fn build_from_other(node: &Node) -> NodeBuilder {
        NodeBuilder::default()
            .path(node.path.clone())
            .chunk(node.chunk.clone())
            .metadata(node.metadata.clone())
            .maybe_vectors(node.vectors.clone())
            .maybe_sparse_vectors(node.sparse_vectors.clone())
            .embed_mode(node.embed_mode)
            .original_size(node.original_size)
            .offset(node.offset)
            .to_owned()
    }

    /// Creates a new instance of `NodeBuilder.`
    pub fn builder() -> NodeBuilder {
        NodeBuilder::default()
    }

    /// Creates a new instance of `Node` with the specified data chunk.
    ///
    /// The other fields are set to their default values.
    pub fn new(chunk: impl Into<String>) -> Node {
        let chunk = chunk.into();
        let original_size = chunk.len();
        Node {
            chunk,
            original_size,
            ..Default::default()
        }
    }

    pub fn with_metadata(&mut self, metadata: impl Into<Metadata>) -> &mut Self {
        self.metadata = metadata.into();
        self
    }

    pub fn with_vectors(
        &mut self,
        vectors: impl Into<HashMap<EmbeddedField, Embedding>>,
    ) -> &mut Self {
        self.vectors = Some(vectors.into());
        self
    }

    pub fn with_sparse_vectors(
        &mut self,
        sparse_vectors: impl Into<HashMap<EmbeddedField, SparseEmbedding>>,
    ) -> &mut Self {
        self.sparse_vectors = Some(sparse_vectors.into());
        self
    }

    /// Creates embeddable data depending on chosen `EmbedMode`.
    ///
    /// # Returns
    ///
    /// Embeddable data mapped to their `EmbeddedField`.
    pub fn as_embeddables(&self) -> Vec<(EmbeddedField, String)> {
        // TODO: Figure out a clever way to do zero copy
        let mut embeddables = Vec::new();

        if self.embed_mode == EmbedMode::SingleWithMetadata || self.embed_mode == EmbedMode::Both {
            embeddables.push((EmbeddedField::Combined, self.combine_chunk_with_metadata()));
        }

        if self.embed_mode == EmbedMode::PerField || self.embed_mode == EmbedMode::Both {
            embeddables.push((EmbeddedField::Chunk, self.chunk.clone()));
            for (name, value) in &self.metadata {
                let value = value
                    .as_str()
                    .map_or_else(|| value.to_string(), ToString::to_string);
                embeddables.push((EmbeddedField::Metadata(name.clone()), value));
            }
        }

        embeddables
    }

    /// Converts the node into an [`self::EmbeddedField::Combined`] type of embeddable.
    ///
    /// This embeddable format consists of the metadata formatted as key-value pairs, each on a new
    /// line, followed by the data chunk.
    ///
    /// # Returns
    ///
    /// A string representing the embeddable format of the node.
    fn combine_chunk_with_metadata(&self) -> String {
        // Metadata formatted by newlines joined with the chunk
        let metadata = self
            .metadata
            .iter()
            .map(|(k, v)| {
                let v = v
                    .as_str()
                    .map_or_else(|| v.to_string(), ToString::to_string);

                format!("{k}: {v}")
            })
            .collect::<Vec<String>>()
            .join("\n");

        format!("{}\n{}", metadata, self.chunk)
    }

    /// Retrieve the identifier of the node.
    ///
    /// Calculates the identifier of the node based on its path and chunk as bytes, returning a
    /// UUID (v3).
    ///
    /// WARN: Does not memoize the id. Use sparingly.
    pub fn id(&self) -> uuid::Uuid {
        let bytes = [self.path.as_os_str().as_bytes(), self.chunk.as_bytes()].concat();

        uuid::Uuid::new_v3(&uuid::Uuid::NAMESPACE_OID, &bytes)
    }
}

impl Hash for Node {
    /// Hashes the node based on its path and chunk.
    ///
    /// This method is used by the `calculate_hash` method to generate a hash value for the node.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.path.hash(state);
        self.chunk.hash(state);
    }
}

impl<T: Into<String>> From<T> for Node {
    fn from(value: T) -> Self {
        Node::new(value)
    }
}

/// Embed mode of the pipeline.
#[derive(Copy, Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub enum EmbedMode {
    #[default]
    /// Embedding Chunk of data combined with Metadata.
    SingleWithMetadata,
    /// Embedding Chunk of data and every Metadata separately.
    PerField,
    /// Embedding Chunk of data and every Metadata separately and Chunk of data combined with
    /// Metadata.
    Both,
}

/// Type of Embeddable stored in model.
#[derive(
    Clone, Default, Serialize, Deserialize, PartialEq, Eq, Hash, strum_macros::Display, Debug,
)]
pub enum EmbeddedField {
    #[default]
    /// Embeddable created from Chunk of data combined with Metadata.
    Combined,
    /// Embeddable created from Chunk of data only.
    Chunk,
    /// Embeddable created from Metadata.
    /// String stores Metadata name.
    #[strum(to_string = "Metadata: {0}")]
    Metadata(String),
}

impl EmbeddedField {
    /// Returns the name of the field when it would be a sparse vector
    pub fn sparse_field_name(&self) -> String {
        format!("{self}_sparse")
    }

    /// Returns the name of the field when it would be a dense vector
    pub fn field_name(&self) -> String {
        format!("{self}")
    }
}

#[allow(clippy::from_over_into)]
impl Into<String> for EmbeddedField {
    fn into(self) -> String {
        self.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case(&EmbeddedField::Combined, ["Combined", "Combined_sparse"])]
    #[test_case(&EmbeddedField::Chunk, ["Chunk", "Chunk_sparse"])]
    #[test_case(&EmbeddedField::Metadata("test".into()), ["Metadata: test", "Metadata: test_sparse"])]
    fn field_name_tests(embedded_field: &EmbeddedField, expected: [&str; 2]) {
        assert_eq!(embedded_field.field_name(), expected[0]);
        assert_eq!(embedded_field.sparse_field_name(), expected[1]);
    }

    #[test]
    fn test_debugging_node_with_utf8_char_boundary() {
        let node = Node::new("🦀".repeat(101));
        // Single char
        let _ = format!("{node:?}");

        // With invalid char boundary
        Node::new("Jürgen".repeat(100));
        let _ = format!("{node:?}");
    }

    #[test]
    fn test_build_from_other_without_vectors() {
        let original_node = Node::new("test_chunk")
            .with_metadata(Metadata::default())
            .with_vectors(HashMap::new())
            .with_sparse_vectors(HashMap::new())
            .to_owned();

        let builder = Node::build_from_other(&original_node);
        let new_node = builder.build().unwrap();

        assert_eq!(original_node, new_node);
    }

    #[test]
    fn test_build_from_other_with_vectors() {
        let mut vectors = HashMap::new();
        vectors.insert(EmbeddedField::Chunk, Embedding::default());

        let mut sparse_vectors = HashMap::new();
        sparse_vectors.insert(
            EmbeddedField::Chunk,
            SparseEmbedding {
                indices: vec![],
                values: vec![],
            },
        );

        let original_node = Node::new("test_chunk")
            .with_metadata(Metadata::default())
            .with_vectors(vectors.clone())
            .with_sparse_vectors(sparse_vectors.clone())
            .to_owned();

        let builder = Node::build_from_other(&original_node);
        let new_node = builder.build().unwrap();

        assert_eq!(original_node, new_node);
    }
}
