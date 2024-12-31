//! Documents are the main data structure that is retrieved via the query pipeline
//!
//! Retrievers are expected to eagerly set any configured metadata on the document, with the same
//! field name used during indexing if applicable.
use std::fmt;

use derive_builder::Builder;
use serde::{Deserialize, Serialize};

use crate::{metadata::Metadata, util::debug_long_utf8};

/// A document represents a single unit of retrieved text
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Builder)]
#[builder(setter(into))]
pub struct Document {
    #[builder(default)]
    metadata: Metadata,
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
            metadata: metadata.unwrap_or_default(),
            content: content.into(),
        }
    }

    pub fn builder() -> DocumentBuilder {
        DocumentBuilder::default()
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::Metadata;

    #[test]
    fn test_document_creation() {
        let content = "Test content";
        let metadata = Metadata::from([("some", "metadata")]);
        let document = Document::new(content, Some(metadata.clone()));

        assert_eq!(document.content(), content);
        assert_eq!(document.metadata(), &metadata);
    }

    #[test]
    fn test_document_default_metadata() {
        let content = "Test content";
        let document = Document::new(content, None);

        assert_eq!(document.content(), content);
        assert_eq!(document.metadata(), &Metadata::default());
    }

    #[test]
    fn test_document_from_str() {
        let content = "Test content";
        let document: Document = content.into();

        assert_eq!(document.content(), content);
        assert_eq!(document.metadata(), &Metadata::default());
    }

    #[test]
    fn test_document_partial_ord() {
        let doc1 = Document::new("A", None);
        let doc2 = Document::new("B", None);

        assert!(doc1 < doc2);
    }

    #[test]
    fn test_document_ord() {
        let doc1 = Document::new("A", None);
        let doc2 = Document::new("B", None);

        assert!(doc1.cmp(&doc2) == std::cmp::Ordering::Less);
    }

    #[test]
    fn test_document_debug() {
        let content = "Test content";
        let document = Document::new(content, None);
        let debug_str = format!("{document:?}");

        assert!(debug_str.contains("Document"));
        assert!(debug_str.contains("metadata"));
        assert!(debug_str.contains("content"));
    }

    #[test]
    fn test_document_to_json() {
        let content = "Test content";
        let metadata = Metadata::from([("some", "metadata")]);
        let document = Document::new(content, Some(metadata.clone()));
        let json_value: serde_json::Value = document.into();

        assert_eq!(json_value["content"], content);
        assert_eq!(json_value["metadata"], serde_json::json!(metadata));
    }

    #[test]
    fn test_document_ref_to_json() {
        let content = "Test content";
        let metadata = Metadata::from([("some", "metadata")]);
        let document = Document::new(content, Some(metadata.clone()));
        let json_value: serde_json::Value = (&document).into();

        assert_eq!(json_value["content"], content);
        assert_eq!(json_value["metadata"], serde_json::json!(metadata));
    }
}
