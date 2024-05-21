use anyhow::{Context as _, Result};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    path::PathBuf,
};

use qdrant_client::{
    client::Payload,
    qdrant::{self, Value},
};

#[derive(Debug, Default, Clone)]
pub struct IngestionNode {
    // TODO: Can we make the ie path + n node the id?
    pub id: Option<u64>,
    pub path: PathBuf,
    pub chunk: String,
    pub vector: Option<Vec<f32>>,
    pub metadata: HashMap<String, String>,
}

impl IngestionNode {
    pub fn as_embeddable(&self) -> String {
        // Metadata formatted by newlines joined with the chunk
        let metadata = self
            .metadata
            .iter()
            .map(|(k, v)| format!("{}: {}", k, v))
            .collect::<Vec<String>>()
            .join("\n");

        format!("{}\n{}", metadata, self.chunk)
    }
}

// TODO: We could also use hashes as the node id instead of uuid?
// That would remove the need for uuid and the extra delete before insert check in storage
// Potential issue there is that if implementation on metadata changes, storage would not update
// ... Or we add metadata to the hash as well?
impl Hash for IngestionNode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.path.hash(state);
        self.chunk.hash(state);
    }
}

impl TryInto<qdrant::PointStruct> for IngestionNode {
    type Error = anyhow::Error;

    fn try_into(mut self) -> Result<qdrant::PointStruct> {
        self.metadata.extend([
            ("path".to_string(), self.path.to_string_lossy().to_string()),
            ("content".to_string(), self.chunk),
        ]);

        // Damn who build this api
        let payload: Payload = self
            .metadata
            .iter()
            .map(|(k, v)| (k.as_str(), Value::from(v.as_str())))
            .collect::<HashMap<&str, Value>>()
            .into();

        Ok(qdrant::PointStruct::new(
            uuid::Uuid::new_v4().to_string(),
            self.vector.context("Vector is not set")?,
            payload,
        ))
    }
}
