//! This module provides functionality to convert an `Node` into a `qdrant::PointStruct`.
//! The conversion is essential for storing data in the Qdrant vector database, which is used
//! for efficient vector similarity search. The module handles metadata augmentation and ensures
//! data compatibility with Qdrant's required format.

use anyhow::{bail, Result};
use std::{
    collections::{HashMap, HashSet},
    string::ToString,
};

use qdrant_client::{
    client::Payload,
    qdrant::{self, Value},
};
use swiftide_core::indexing::EmbeddedField;

use super::NodeWithVectors;

/// Implements the `TryInto` trait to convert an `NodeWithVectors` into a `qdrant::PointStruct`.
/// This conversion is necessary for storing the node in the Qdrant vector database.
impl TryInto<qdrant::PointStruct> for NodeWithVectors {
    type Error = anyhow::Error;

    /// Converts the `Node` into a `qdrant::PointStruct`.
    ///
    /// # Errors
    ///
    /// Returns an error if the vector is not set in the `Node`.
    ///
    /// # Returns
    ///
    /// A `Result` which is `Ok` if the conversion is successful, containing the `qdrant::PointStruct`.
    /// If the conversion fails, it returns an `anyhow::Error`.
    fn try_into(self) -> Result<qdrant::PointStruct> {
        let mut node = self.node;
        // Calculate a unique identifier for the node.
        let id = node.calculate_hash();

        // Extend the metadata with additional information.
        node.metadata.extend([
            ("path", node.path.to_string_lossy().to_string()),
            ("content", node.chunk),
            ("last_updated_at", chrono::Utc::now().to_rfc3339()),
        ]);

        // Create a payload compatible with Qdrant's API.
        let payload: Payload = node
            .metadata
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str(),
                    Value::from(
                        v.as_str()
                            .map_or_else(|| v.to_string(), ToString::to_string),
                    ),
                )
            })
            .collect::<HashMap<&str, Value>>()
            .into();

        let Some(vectors) = node.vectors else {
            bail!("Node without vectors")
        };
        let vectors = try_create_vectors(&self.vector_fields, vectors)?;

        // Construct the `qdrant::PointStruct` and return it.
        Ok(qdrant::PointStruct::new(id, vectors, payload))
    }
}

fn try_create_vectors(
    vector_fields: &HashSet<EmbeddedField>,
    vectors: HashMap<EmbeddedField, Vec<f32>>,
) -> Result<qdrant::Vectors> {
    if vectors.is_empty() {
        bail!("Node with empty vectors")
    } else if vectors.len() == 1 {
        let Some(vector) = vectors.into_values().next() else {
            bail!("Node has no vector entry")
        };
        return Ok(vector.into());
    }
    let vectors = vectors
        .into_iter()
        .filter(|(field, _)| vector_fields.contains(field))
        .map(|(vector_type, vector)| (vector_type.to_string(), vector))
        .collect::<HashMap<String, Vec<f32>>>();
    Ok(vectors.into())
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use qdrant_client::qdrant::{
        vectors::VectorsOptions, NamedVectors, PointId, PointStruct, Value, Vector, Vectors,
    };
    use swiftide_core::indexing::{EmbeddedField, Metadata, Node};
    use test_case::test_case;

    use crate::qdrant::indexing_node::NodeWithVectors;

    #[test_case(
        Node { id: Some(1), path: "/path".into(), chunk: "data".into(),
            vectors: Some(HashMap::from([(EmbeddedField::Chunk, vec![1.0])])),
            original_size: 4,
            offset: 0,
            metadata: Metadata::from([("m1", "mv1")]),
            embed_mode: swiftide_core::indexing::EmbedMode::SingleWithMetadata
        },
        HashSet::from([EmbeddedField::Combined]),
        PointStruct { id: Some(PointId::from(6_516_159_902_038_153_111)), payload: HashMap::from([
            ("content".into(), Value::from("data")),
            ("path".into(), Value::from("/path")),
            ("m1".into(), Value::from("mv1"))]),
            vectors: Some(Vectors { vectors_options: Some(VectorsOptions::Vector(Vector { data: vec![1.0], ..Default::default()} )) })
        };
        "Node with single vector creates struct with unnamed vector"
    )]
    #[test_case(
        Node { id: Some(1), path: "/path".into(), chunk: "data".into(),
            vectors: Some(HashMap::from([
                (EmbeddedField::Chunk, vec![1.0]),
                (EmbeddedField::Metadata("m1".into()), vec![2.0])
            ])),
            metadata: Metadata::from([("m1", "mv1")]),
            embed_mode: swiftide_core::indexing::EmbedMode::PerField,
            original_size: 4,
            offset: 0
        },
        HashSet::from([EmbeddedField::Chunk, EmbeddedField::Metadata("m1".into())]),
        PointStruct { id: Some(PointId::from(6_516_159_902_038_153_111)), payload: HashMap::from([
            ("content".into(), Value::from("data")),
            ("path".into(), Value::from("/path")),
            ("m1".into(), Value::from("mv1"))]),
            vectors: Some(Vectors { vectors_options: Some(VectorsOptions::Vectors(NamedVectors { vectors: HashMap::from([
                ("Chunk".into(), qdrant_client::qdrant::Vector {
                    data: vec![1.0], ..Default::default()
                }),
                ("Metadata: m1".into(), qdrant_client::qdrant::Vector {
                    data: vec![2.0], ..Default::default()
                })
            ]) })) })
        };
        "Node with multiple vectors creates struct with named vectors"
    )]
    #[test_case(
        Node { id: Some(1), path: "/path".into(), chunk: "data".into(),
            vectors: Some(HashMap::from([
                (EmbeddedField::Chunk, vec![1.0]),
                (EmbeddedField::Combined, vec![1.0]),
                (EmbeddedField::Metadata("m1".into()), vec![1.0]),
                (EmbeddedField::Metadata("m2".into()), vec![2.0])
            ])),
            metadata: Metadata::from([("m1", "mv1"), ("m2", "mv2")]),
            embed_mode: swiftide_core::indexing::EmbedMode::Both,
            original_size: 4,
            offset: 0,
        },
        HashSet::from([EmbeddedField::Combined]),
        PointStruct { id: Some(PointId::from(6_516_159_902_038_153_111)), payload: HashMap::from([
            ("content".into(), Value::from("data")),
            ("path".into(), Value::from("/path")),
            ("m1".into(), Value::from("mv1")),
            ("m2".into(), Value::from("mv2"))]),
            vectors: Some(Vectors { vectors_options: Some(VectorsOptions::Vectors(NamedVectors { vectors: HashMap::from([
                ("Combined".into(), qdrant_client::qdrant::Vector {
                    data: vec![1.0], ..Default::default()
                })
            ]) })) })
        };
        "Storing only `Combined` vector. Skipping other vectors."
    )]
    fn try_into_point_struct_test(
        node: Node,
        vector_fields: HashSet<EmbeddedField>,
        mut expected_point: PointStruct,
    ) {
        let node = NodeWithVectors::new(node, vector_fields);
        let point: PointStruct = node.try_into().expect("Can create PointStruct");

        // patch last_update_at field to avoid test failure because of time difference
        let last_updated_at_key = "last_updated_at";
        let last_updated_at = point
            .payload
            .get(last_updated_at_key)
            .expect("Has autogenerated `last_updated_at` field.");
        expected_point
            .payload
            .insert(last_updated_at_key.into(), last_updated_at.clone());

        assert_eq!(point, expected_point);
    }
}
