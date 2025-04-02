use anyhow::{Context as _, Result};
use itertools::Itertools;
use std::sync::Arc;

use async_trait::async_trait;
use derive_builder::Builder;
use fastembed::{RerankInitOptions, TextRerank};
use swiftide_core::{
    querying::{states, Query},
    TransformResponse,
};

const TOP_K: usize = 10;

// NOTE: If ever more rerank models are added (outside fastembed). This should be refactored to a
// generic implementation with textrerank behind an interface.
//
// NOTE: Additionally, controlling what gets used for reranking from the query side (maybe not just
// the original?), is also something to be said for. The usecase hasn't popped up yet.

/// Reranking with [`fastembed::TextRerank`] in a query pipeline.
///
/// Uses the original user query to compare with the retrieved documents. Then updates the query
/// with the `TOP_K` documents with the highest rerank score.
///
/// Can be customized with any rerank model from `fastembed` and the number of top documents to
/// return. Optionally you can provide a template to render the document before reranking.
#[derive(Clone, Builder)]
pub struct Rerank {
    /// The reranker model from [`Fastembed`]
    #[builder(
        default = "Arc::new(TextRerank::try_new(RerankInitOptions::default()).expect(\"Failed to build default rerank from Fastembed.rs\"))",
        setter(into)
    )]
    model: Arc<TextRerank>,

    /// The number of top documents returned by the reranker.
    #[builder(default = TOP_K)]
    top_k: usize,

    /// Optionally a template can be provided to render the document
    /// before reranking. I.e. to include metadata in the reranking.
    ///
    /// Available variables are `metadata` and `content`.
    ///
    /// Templates are rendered using Tera.
    #[builder(default = None)]
    document_template: Option<String>,

    /// The rerank batch size to use. Defaults to the `Fastembed` default.
    #[builder(default = None)]
    model_batch_size: Option<usize>,
}

impl std::fmt::Debug for Rerank {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rerank").finish()
    }
}

impl Rerank {
    pub fn builder() -> RerankBuilder {
        RerankBuilder::default()
    }
}

impl Default for Rerank {
    fn default() -> Self {
        Self {
            model: Arc::new(
                TextRerank::try_new(RerankInitOptions::default())
                    .expect("Failed to build default rerank from Fastembed.rs"),
            ),
            top_k: TOP_K,
            document_template: None,
            model_batch_size: None,
        }
    }
}

#[async_trait]
impl TransformResponse for Rerank {
    async fn transform_response(
        &self,
        query: Query<states::Retrieved>,
    ) -> Result<Query<states::Retrieved>> {
        let mut query = query;

        let current_documents = std::mem::take(&mut query.documents);

        let docs_for_rerank = if let Some(template) = &self.document_template {
            current_documents
                .iter()
                .map(|doc| {
                    let context = tera::Context::from_serialize(doc)?;
                    tera::Tera::one_off(template, &context, false)
                        .context("Failed to render template")
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            current_documents
                .iter()
                .map(|doc| doc.content().to_string())
                .collect()
        };

        let reranked_documents = self
            .model
            .rerank(
                query.original(),
                docs_for_rerank.iter().map(String::as_ref).collect(),
                false,
                self.model_batch_size,
            )
            .map_err(|e| anyhow::anyhow!("Failed to rerank documents: {:?}", e))?
            .iter()
            .take(self.top_k)
            .map(|r| current_documents[r.index].clone())
            .collect_vec();

        query.documents = reranked_documents;

        Ok(query)
    }
}

#[cfg(test)]
mod tests {
    use swiftide_core::{document::Document, indexing::Metadata};

    use super::*;

    #[tokio::test]
    async fn test_rerank_transform_response() {
        // Test reranking without a template
        let rerank = Rerank::builder().top_k(1).build().unwrap();

        let documents = vec!["content1", "content2", "content3"]
            .into_iter()
            .map(Into::into)
            .collect_vec();

        let query = Query::builder()
            .original("What is the capital of france?")
            .state(states::Retrieved)
            .documents(documents)
            .build()
            .unwrap();

        let result = rerank.transform_response(query).await;

        assert!(result.is_ok());
        let transformed_query = result.unwrap();
        assert_eq!(transformed_query.documents.len(), 1);

        // Test reranking with a template
        let rerank = Rerank::builder()
            .top_k(1)
            .document_template(Some("{{ metadata.title }}".to_string()))
            .build()
            .unwrap();

        let metadata = Metadata::from([("title", "Title")]);

        let documents = vec!["content1", "content2", "content3"]
            .into_iter()
            .map(|content| Document::new(content, Some(metadata.clone())))
            .collect_vec();

        let query = Query::builder()
            .original("What is the capital of france?")
            .state(states::Retrieved)
            .documents(documents)
            .build()
            .unwrap();

        let result = rerank.transform_response(query).await;

        assert!(result.is_ok());
        let transformed_query = result.unwrap();
        assert_eq!(transformed_query.documents.len(), 1);
    }
}
