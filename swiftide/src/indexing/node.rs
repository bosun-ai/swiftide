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
    collections::{BTreeMap, HashMap},
    fmt::Debug,
    hash::{Hash, Hasher},
    path::PathBuf,
};

use itertools::Itertools;
use serde::{Deserialize, Serialize};

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
    pub vectors: Option<HashMap<EmbeddedField, Vec<f32>>>,
    /// Metadata associated with the node.
    pub metadata: BTreeMap<String, String>,
    /// Mode of embedding data Chunk and Metadata
    pub embed_mode: EmbedMode,
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
                    .map(HashMap::iter)
                    .flatten()
                    .map(|(embed_type, vec)| format!("'{embed_type}': {}", vec.len()))
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
        Node {
            chunk: chunk.into(),
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
            for (name, value) in self.metadata.iter() {
                embeddables.push((EmbeddedField::Metadata(name.clone()), value.clone()));
            }
        }

        embeddables
    }

    /// Converts the node into an [self::EmbeddedField::Combined] type of embeddable.
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
            .map(|(k, v)| format!("{k}: {v}"))
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
/// See also [super::pipeline::Pipeline::with_embed_mode].
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
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq, Hash, strum_macros::Display)]
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
