use std::sync::Arc;

use crate::{EmbeddingModel, SimplePrompt};

#[derive(Debug, Default, Clone)]
pub struct IndexingDefaults(Arc<IndexingDefaultsInner>);

#[derive(Debug, Default)]
pub struct IndexingDefaultsInner {
    simple_prompt: Option<Box<dyn SimplePrompt>>,
    embedding_model: Option<Box<dyn EmbeddingModel>>,
}

impl IndexingDefaults {
    pub fn simple_prompt(&self) -> Option<&dyn SimplePrompt> {
        self.0.simple_prompt.as_deref()
    }
    pub fn embedding_model(&self) -> Option<&dyn EmbeddingModel> {
        self.0.embedding_model.as_deref()
    }
}
