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
    path::PathBuf,
};

use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::{metadata::Metadata, Embedding, SparseEmbedding};

/// Represents a unit of data in the indexing process.
///
/// `Node` encapsulates all necessary information for a single unit of data being processed
/// in the indexing pipeline. It includes fields for an identifier, file path, data chunk, optional
/// vector representation, and metadata.
#[derive(Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct Node {
    /// Optional identifier for the node.
    pub id: Option<u64>,
    /// File path associated with the node.
    pub path: PathBuf,
    /// Data chunk contained in the node.
    pub chunk: String,
    /// Optional vector representation of embedded data.
    pub vectors: Option<HashMap<EmbeddedField, Embedding>>,
    /// Optional sparse vector representation of embedded data.
    pub sparse_vectors: Option<HashMap<EmbeddedField, SparseEmbedding>>,
    /// Metadata associated with the node.
    pub metadata: Metadata,
    /// Mode of embedding data Chunk and Metadata
    pub embed_mode: EmbedMode,
    /// Size of the input this node was originally derived from in bytes
    pub original_size: usize,
    /// Offset of the chunk relative to the start of the input this node was originally derived from in bytes
    pub offset: usize,
}

impl Debug for Node {
    /// Formats the node for debugging purposes.
    ///
    /// This method is used to provide a human-readable representation of the node when debugging.
    /// The vector field is displayed as the number of elements in the vector if present.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Node")
            .field("id", &self.id)
            .field("path", &self.path)
            .field("chunk", &self.chunk)
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

    /// Creates embeddable data depending on chosen `EmbedMode`.
    ///
    /// # Returns
    ///
    /// Embeddable data mapped to their `EmbeddedField`.
    pub fn as_embeddables(&self) -> Vec<(EmbeddedField, String)> {
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
    /// This embeddable format consists of the metadata formatted as key-value pairs, each on a new line,
    /// followed by the data chunk.
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

    /// Calculates a hash value for the node based on its path and chunk.
    ///
    /// The hash value is calculated using the default hasher provided by the standard library.
    ///
    /// # Returns
    ///
    /// A 64-bit hash value representing the node.
    pub fn calculate_hash(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
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

/// Embed mode of the pipeline.
///
/// See also [`super::pipeline::Pipeline::with_embed_mode`].
#[derive(Copy, Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub enum EmbedMode {
    #[default]
    /// Embedding Chunk of data combined with Metadata.
    SingleWithMetadata,
    /// Embedding Chunk of data and every Metadata separately.
    PerField,
    /// Embedding Chunk of data and every Metadata separately and Chunk of data combined with Metadata.
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
}
