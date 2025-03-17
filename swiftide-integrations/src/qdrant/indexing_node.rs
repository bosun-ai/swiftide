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
use swiftide_core::{indexing::EmbeddedField, Embedding, SparseEmbedding};

use super::NodeWithVectors;

/// Implements the `TryInto` trait to convert an `NodeWithVectors` into a `qdrant::PointStruct`.
/// This conversion is necessary for storing the node in the Qdrant vector database.
impl TryInto<qdrant::PointStruct> for NodeWithVectors<'_> {
    type Error = anyhow::Error;

    /// Converts the `Node` into a `qdrant::PointStruct`.
    ///
    /// # Errors
    ///
    /// Returns an error if the vector is not set in the `Node`.
    ///
    /// # Returns
    ///
    /// A `Result` which is `Ok` if the conversion is successful, containing the
    /// `qdrant::PointStruct`. If the conversion fails, it returns an `anyhow::Error`.
    fn try_into(self) -> Result<qdrant::PointStruct> {
        let node = self.node;
        // Calculate a unique identifier for the node.
        let id = node.id();

        // Extend the metadata with additional information.
        // TODO: The node is already cloned in the `NodeWithVectors` constructor.
        // Then additional data is added to the metadata, including the full chunk
        // Data is then taken as ref and reassigned. Seems like a lot of needless allocations

        // Create a payload compatible with Qdrant's API.
        let mut payload: Payload = node
            .metadata
            .iter()
            .map(|(k, v)| (k.clone(), Value::from(v.clone())))
            .collect::<HashMap<String, Value>>()
            .into();

        payload.insert("path", node.path.to_string_lossy().to_string());
        payload.insert("content", node.chunk.clone());
        payload.insert(
            "last_updated_at",
            Value::from(chrono::Utc::now().to_rfc3339()),
        );

        let Some(vectors) = node.vectors.clone() else {
            bail!("Node without vectors")
        };
        let vectors =
            try_create_vectors(&self.vector_fields, vectors, node.sparse_vectors.clone())?;

        // Construct the `qdrant::PointStruct` and return it.
        Ok(qdrant::PointStruct::new(id.to_string(), vectors, payload))
    }
}

fn try_create_vectors(
    vector_fields: &HashSet<&EmbeddedField>,
    vectors: HashMap<EmbeddedField, Embedding>,
    sparse_vectors: Option<HashMap<EmbeddedField, SparseEmbedding>>,
) -> Result<qdrant::Vectors> {
    if vectors.is_empty() {
        bail!("Node with empty vectors")
    } else if vectors.len() == 1 && sparse_vectors.is_none() {
        let Some(vector) = vectors.into_values().next() else {
            bail!("Node has no vector entry")
        };
        return Ok(vector.into());
    }
    let mut qdrant_vectors = qdrant::NamedVectors::default();

    for (field, vector) in vectors {
        if !vector_fields.contains(&field) {
            continue;
        }
        qdrant_vectors = qdrant_vectors.add_vector(field.to_string(), vector);
    }

    if let Some(sparse_vectors) = sparse_vectors {
        for (field, sparse_vector) in sparse_vectors {
            if !vector_fields.contains(&field) {
                continue;
            }

            qdrant_vectors = qdrant_vectors.add_vector(
                format!("{field}_sparse"),
                qdrant::Vector::new_sparse(
                    sparse_vector.indices.into_iter().collect::<Vec<_>>(),
                    sparse_vector.values,
                ),
            );
        }
    }

    Ok(qdrant_vectors.into())
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use qdrant_client::qdrant::PointStruct;
    use swiftide_core::indexing::{EmbeddedField, Node};
    use test_case::test_case;

    use crate::qdrant::indexing_node::NodeWithVectors;
    use pretty_assertions::assert_eq;

    static EXPECTED_UUID: &str = "d42d252d-671d-37ef-a157-8e85d0710610";

    #[test_case(
        Node::builder()
            .path("/path")
            .chunk("data")
            .vectors([(EmbeddedField::Chunk, vec![1.0])])
            .metadata([("m1", "mv1")])
            .embed_mode(swiftide_core::indexing::EmbedMode::SingleWithMetadata)
            .build().unwrap()
        ,
        HashSet::from([EmbeddedField::Combined]),
        PointStruct::new(EXPECTED_UUID, vec![1.0], HashMap::from([
            ("content", "data".into()),
            ("path", "/path".into()),
            ("m1", "mv1".into())])
        );
        "Node with single vector creates struct with unnamed vector"
    )]
    #[test_case(
        Node::builder()
            .path("/path")
            .chunk("data")
            .vectors([
                (EmbeddedField::Chunk, vec![1.0]),
                (EmbeddedField::Metadata("m1".into()), vec![2.0])
            ])
            .metadata([("m1", "mv1")])
            .embed_mode(swiftide_core::indexing::EmbedMode::PerField)
            .build().unwrap(),
        HashSet::from([EmbeddedField::Chunk, EmbeddedField::Metadata("m1".into())]),
        PointStruct::new(EXPECTED_UUID, HashMap::from([
                ("Chunk".to_string(), vec![1.0]),
                ("Metadata: m1".to_string(), vec![2.0])
            ]),
            HashMap::from([
                ("content", "data".into()),
                ("path", "/path".into()),
                ("m1", "mv1".into())])
        );
        "Node with multiple vectors creates struct with named vectors"
    )]
    #[test_case(
        Node::builder()
            .path("/path")
            .chunk("data")
            .vectors([
                (EmbeddedField::Chunk, vec![1.0]),
                (EmbeddedField::Combined, vec![1.0]),
                (EmbeddedField::Metadata("m1".into()), vec![1.0]),
                (EmbeddedField::Metadata("m2".into()), vec![2.0])
            ])
            .metadata([("m1", "mv1"), ("m2", "mv2")])
            .embed_mode(swiftide_core::indexing::EmbedMode::Both)
            .build().unwrap(),
        HashSet::from([EmbeddedField::Combined]),
        PointStruct::new(EXPECTED_UUID,
            HashMap::from([
                ("Combined".to_string(), vec![1.0]),
            ]),
            HashMap::from([
                ("content", "data".into()),
                ("path", "/path".into()),
                ("m1", "mv1".into()),
                ("m2", "mv2".into())])
        );
        "Storing only `Combined` vector. Skipping other vectors."
    )]
    #[allow(clippy::needless_pass_by_value)]
    fn try_into_point_struct_test(
        node: Node,
        vector_fields: HashSet<EmbeddedField>,
        mut expected_point: PointStruct,
    ) {
        let node = NodeWithVectors::new(&node, vector_fields.iter().collect());
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

        assert_eq!(point.id, expected_point.id);
        assert_eq!(point.payload, expected_point.payload);
        assert_eq!(point.vectors, expected_point.vectors);
    }
}
