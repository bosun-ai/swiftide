use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{metadata::Metadata, util::debug_long_utf8};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    metadata: Option<Metadata>,
    content: String,
}

impl From<Document> for serde_json::Value {
    fn from(document: Document) -> Self {
        serde_json::json!({
            "metadata": document.metadata,
            "content": document.content,
        })
    }
}

impl From<&Document> for serde_json::Value {
    fn from(document: &Document) -> Self {
        serde_json::json!({
            "metadata": document.metadata,
            "content": document.content,
        })
    }
}

impl PartialOrd for Document {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.content.cmp(&other.content))
    }
}

impl Ord for Document {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.content.cmp(&other.content)
    }
}

impl fmt::Debug for Document {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Document")
            .field("metadata", &self.metadata)
            .field("content", &debug_long_utf8(&self.content, 100))
            .finish()
    }
}

impl<T: AsRef<str>> From<T> for Document {
    fn from(value: T) -> Self {
        Document::new(value.as_ref(), None)
    }
}

impl Document {
    pub fn new(content: impl Into<String>, metadata: Option<Metadata>) -> Self {
        Self {
            metadata,
            content: content.into(),
        }
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn metadata(&self) -> Option<&Metadata> {
        self.metadata.as_ref()
    }
}
