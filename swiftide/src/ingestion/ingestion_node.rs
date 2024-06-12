use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    path::PathBuf,
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
