//! This module provides functionality to convert an `Node` into a `qdrant::PointStruct`.
//! The conversion is essential for storing data in the Qdrant vector database, which is used
//! for efficient vector similarity search. The module handles metadata augmentation and ensures
//! data compatibility with Qdrant's required format.

use anyhow::{bail, Result};
use std::collections::HashMap;

use crate::ingestion::{EmbeddableType, Node};
use qdrant_client::{
    client::Payload,
    qdrant::{self, Value},
};

/// Implements the `TryInto` trait to convert an `Node` into a `qdrant::PointStruct`.
/// This conversion is necessary for storing the node in the Qdrant vector database.
impl TryInto<qdrant::PointStruct> for Node {
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
    fn try_into(mut self) -> Result<qdrant::PointStruct> {
        // Calculate a unique identifier for the node.
        let id: u64 = self.calculate_hash();

        // Extend the metadata with additional information.
        self.metadata.extend([
            ("path".to_string(), self.path.to_string_lossy().to_string()),
            ("content".to_string(), self.chunk),
            (
                "last_updated_at".to_string(),
                chrono::Utc::now().to_rfc3339(),
            ),
        ]);

        // Create a payload compatible with Qdrant's API.
        let payload: Payload = self
            .metadata
            .iter()
            .map(|(k, v)| (k.as_str(), Value::from(v.as_str())))
            .collect::<HashMap<&str, Value>>()
            .into();

        let Some(vectors) = self.vectors else {
            bail!("Node without vectors")
        };
        let vectors = try_create_vectors(vectors)?;

        // Construct the `qdrant::PointStruct` and return it.
        Ok(qdrant::PointStruct::new(id, vectors, payload))
    }
}

fn try_create_vectors(vectors: HashMap<EmbeddableType, Vec<f32>>) -> Result<qdrant::Vectors> {
    if vectors.is_empty() {
        bail!("Node with empty vectors")
    } else if vectors.len() == 1 {
        let vector = vectors.into_values().next().expect("Node has vector entry");
        return Ok(vector.into());
    }
    let vectors = vectors
        .into_iter()
        .map(|(vector_type, vector)| (vector_type.to_string(), vector))
        .collect::<HashMap<String, Vec<f32>>>();
    Ok(vectors.into())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use qdrant_client::qdrant::{
        vectors::VectorsOptions, NamedVectors, PointId, PointStruct, Value, Vector, Vectors,
    };
    use test_case::test_case;

    use crate::ingestion::{EmbeddableType, Node};

    #[test_case(
        Node { id: Some(1), path: "/path".into(), chunk: "data".into(),
            vectors: Some(HashMap::from([(EmbeddableType::Chunk, vec![1.0])])),
            metadata: BTreeMap::from([("m1".into(), "mv1".into())]),
            embed_mode: crate::ingestion::EmbedMode::SingleWithMetadata
        },
        PointStruct { id: Some(PointId::from(6516159902038153111)), payload: HashMap::from([
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
                (EmbeddableType::Chunk, vec![1.0]),
                (EmbeddableType::Metadata("m1".into()), vec![2.0])
            ])),
            metadata: BTreeMap::from([("m1".into(), "mv1".into())]),
            embed_mode: crate::ingestion::EmbedMode::PerField
        },
        PointStruct { id: Some(PointId::from(6516159902038153111)), payload: HashMap::from([
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
                // missing chunk and non existing Metadata vector
                (EmbeddableType::Metadata("m2".into()), vec![1.0]),
                (EmbeddableType::Metadata("m3".into()), vec![2.0])
            ])),
            metadata: BTreeMap::from([("m1".into(), "mv1".into())]),
            embed_mode: crate::ingestion::EmbedMode::SingleWithMetadata
        },
        PointStruct { id: Some(PointId::from(6516159902038153111)), payload: HashMap::from([
            ("content".into(), Value::from("data")),
            ("path".into(), Value::from("/path")),
            ("m1".into(), Value::from("mv1"))]), 
            vectors: Some(Vectors { vectors_options: Some(VectorsOptions::Vectors(NamedVectors { vectors: HashMap::from([
                ("Metadata: m2".into(), qdrant_client::qdrant::Vector {
                    data: vec![1.0], ..Default::default()
                }),
                ("Metadata: m3".into(), qdrant_client::qdrant::Vector {
                    data: vec![2.0], ..Default::default()
                })
            ]) })) })
        };
        "Property `embed_mode` and `metadata` are ignored. Any map of vectors will be converted into named vectors."
    )]
    fn try_into_point_struct_test(node: Node, mut expected_point: PointStruct) {
        let point: PointStruct = node.try_into().expect("Can create PointStruct");

        // patch last_update_at field
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
