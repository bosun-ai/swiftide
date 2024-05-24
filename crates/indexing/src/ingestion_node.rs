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

    pub fn calculate_hash(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

impl Hash for IngestionNode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.path.hash(state);
        self.chunk.hash(state);
    }
}

impl TryInto<qdrant::PointStruct> for IngestionNode {
    type Error = anyhow::Error;

    fn try_into(mut self) -> Result<qdrant::PointStruct> {
        let id = self.calculate_hash();

        self.metadata.extend([
            ("path".to_string(), self.path.to_string_lossy().to_string()),
            ("content".to_string(), self.chunk),
            (
                "last_updated_at".to_string(),
                chrono::Utc::now().to_rfc3339(),
            ),
        ]);

        // Damn who build this api
        let payload: Payload = self
            .metadata
            .iter()
            .map(|(k, v)| (k.as_str(), Value::from(v.as_str())))
            .collect::<HashMap<&str, Value>>()
            .into();

        Ok(qdrant::PointStruct::new(
            id,
            self.vector.context("Vector is not set")?,
            payload,
        ))
    }
}
