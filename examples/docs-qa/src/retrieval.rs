//! Workaround for a Swiftide 0.33 bug in Qdrant retrieval.
//!
//! `Qdrant` turns the stored `content` payload into a string with `Value::to_string`, which
//! formats string values with `Debug`. Every retrieved chunk then arrives wrapped in quotes with
//! escaped newlines (`"## Title\n\nBody"`), and that is what the reranker, the LLM and the Ragas
//! export see. This wrapper restores the original text right after retrieval, so everything
//! downstream (including the evaluator) gets clean markdown.

use anyhow::Result;
use async_trait::async_trait;
use swiftide::{
    integrations::qdrant::{Qdrant, qdrant_client::qdrant::Filter},
    query::{Document, Query, Retrieve, search_strategies::HybridSearch, states},
};

/// Qdrant, returning documents with their content as it was indexed.
#[derive(Debug, Clone)]
pub struct CleanQdrant(Qdrant);

impl From<Qdrant> for CleanQdrant {
    fn from(qdrant: Qdrant) -> Self {
        Self(qdrant)
    }
}

#[async_trait]
impl Retrieve<HybridSearch<Filter>> for CleanQdrant {
    async fn retrieve(
        &self,
        search_strategy: &HybridSearch<Filter>,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>> {
        let mut retrieved = self.0.retrieve(search_strategy, query).await?;

        for document in retrieved.documents_mut() {
            if let Some(content) = unquote(document.content()) {
                *document = Document::new(content, Some(document.metadata().clone()));
            }
        }

        Ok(retrieved)
    }
}

/// Reverses the `Debug` quoting of a string; `None` when the text was not quoted.
///
/// JSON string escapes cover what `Debug` produces for markdown (`\n`, `\t`, `\"`, `\\`).
fn unquote(content: &str) -> Option<String> {
    content
        .starts_with('"')
        .then(|| serde_json::from_str(content).ok())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_quoted_content_is_restored() {
        let original = "## Tools\n\nUse `\"read_file\"` with a C:\\path\t.";
        let quoted = format!("{original:?}");

        assert_eq!(unquote(&quoted).as_deref(), Some(original));
    }

    #[test]
    fn plain_content_is_left_alone() {
        assert_eq!(unquote("## Tools"), None);
    }
}
