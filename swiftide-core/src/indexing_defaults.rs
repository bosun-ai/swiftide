use std::sync::Arc;

use crate::SimplePrompt;

#[derive(Debug, Default, Clone)]
pub struct IndexingDefaults(Arc<IndexingDefaultsInner>);

#[derive(Debug, Default)]
pub struct IndexingDefaultsInner {
    simple_prompt: Option<Box<dyn SimplePrompt>>,
}

impl IndexingDefaults {
    pub fn simple_prompt(&self) -> Option<&dyn SimplePrompt> {
        self.0.simple_prompt.as_deref()
    }

    pub fn from_simple_prompt(simple_prompt: Box<dyn SimplePrompt>) -> Self {
        Self(Arc::new(IndexingDefaultsInner {
            simple_prompt: Some(simple_prompt),
        }))
    }
}
