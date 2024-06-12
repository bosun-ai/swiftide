use anyhow::{Context as _, Result};
use std::collections::HashMap;

use crate::ingestion::IngestionNode;
use qdrant_client::{
    client::Payload,
    qdrant::{self, Value},
};

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
