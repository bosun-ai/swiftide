//! Querying: hybrid search on Qdrant, rerank locally, answer with an LLM.

use anyhow::Result;
use swiftide::{
    indexing::transformers::metadata_title,
    integrations::{
        fastembed::Rerank,
        qdrant::{Qdrant, qdrant_client::qdrant::Filter},
    },
    query::{
        self, Document, Query, answers, evaluators::ragas::Ragas, query_transformers,
        search_strategies::HybridSearch, states,
    },
};

use crate::{
    docs::PAGE_TITLE,
    models::{Embedders, RetryingModel},
    retrieval::CleanQdrant,
};

/// Candidates fetched per search (dense and sparse each), fused with reciprocal rank fusion.
const CANDIDATES: u64 = 20;
/// Chunks kept after reranking; this is what the LLM sees.
const CONTEXT_CHUNKS: usize = 10;
/// Qdrant stores each chunk's source path under this payload key.
const PATH_METADATA: &str = "path";

// ANCHOR: answer_templates
// The answer prompt receives the question and the rendered documents; each document is rendered
// with its page title and the chunk title written during indexing.
const ANSWER_PROMPT: &str = "You answer questions about Swiftide, a Rust library for building \
LLM applications, using excerpts from its documentation.

Question: {{ question }}

## Instructions
* Use only the documentation excerpts below. If they do not answer the question, say so plainly.
* Start with the direct answer in one or two sentences, then add supporting details.
* Only when the question asks how to do something, include the feature flags, commands or a \
short Rust snippet from the excerpts. Never invent APIs.
* Format the answer as markdown and keep it under 250 words. Do not refer to \"the excerpts\".

## Documentation excerpts

{{ documents }}
";

const DOCUMENT_TEMPLATE: &str = "### {{ metadata.Page | default(value=\"\") }}: \
{{ metadata.Title | default(value=\"\") }}
{{ content }}";

// The reranker scores the same title-first rendering. A chunk that is mostly code says little
// about what it shows, so its page and the title written during indexing go first.
const RERANK_TEMPLATE: &str = "{{ metadata.Page | default(value=\"\") }}: \
{{ metadata.Title | default(value=\"\") }}
{{ content }}";
// ANCHOR_END: answer_templates

/// Everything needed to answer questions over the indexed docs.
pub struct DocsQa {
    qdrant: CleanQdrant,
    embedders: Embedders,
    reranker: Rerank,
    answerer: answers::Simple,
}

type AnsweringPipeline = query::Pipeline<'static, HybridSearch<Filter>, states::Answered>;

impl DocsQa {
    pub fn new(qdrant: Qdrant, embedders: Embedders, llm: RetryingModel) -> Result<Self> {
        // ANCHOR: answerer
        let reranker = Rerank::builder()
            .top_k(CONTEXT_CHUNKS)
            .document_template(Some(RERANK_TEMPLATE.to_string()))
            .build()?;

        let answerer = answers::Simple::builder()
            .client(llm)
            .prompt_template(ANSWER_PROMPT.into())
            .document_template(DOCUMENT_TEMPLATE)
            .build()?;
        // ANCHOR_END: answerer

        Ok(Self {
            qdrant: qdrant.into(),
            embedders,
            reranker,
            answerer,
        })
    }

    pub async fn ask(&self, question: &str) -> Result<Answer> {
        let answered = self
            .answer_with(Self::hybrid_search())
            .query(question)
            .await?;
        Ok(Answer(answered))
    }

    /// The same pipeline as [`Self::ask`], recording every retrieval and answer in `ragas`.
    pub fn evaluated_pipeline(&self, ragas: Ragas) -> AnsweringPipeline {
        self.answer_with(Self::hybrid_search().evaluate_with(ragas))
    }

    fn hybrid_search() -> query::Pipeline<'static, HybridSearch<Filter>> {
        // ANCHOR: hybrid
        query::Pipeline::from_search_strategy(
            HybridSearch::default()
                .with_top_n(CANDIDATES)
                .with_top_k(CANDIDATES),
        )
        // ANCHOR_END: hybrid
    }

    fn answer_with(
        &self,
        pipeline: query::Pipeline<'static, HybridSearch<Filter>>,
    ) -> AnsweringPipeline {
        // ANCHOR: query_pipeline
        pipeline
            // Hybrid search needs both a dense and a sparse embedding of the question
            .then_transform_query(query_transformers::Embed::from_client(
                self.embedders.dense.clone(),
            ))
            .then_transform_query(query_transformers::SparseEmbed::from_client(
                self.embedders.sparse.clone(),
            ))
            .then_retrieve(self.qdrant.clone())
            // ANCHOR: rerank
            // A local cross-encoder reorders the fused candidates; only the best few remain
            .then_transform_response(self.reranker.clone())
            // ANCHOR_END: rerank
            .then_answer(self.answerer.clone())
        // ANCHOR_END: query_pipeline
    }
}

/// An answered question, with the documents it was answered from.
pub struct Answer(Query<states::Answered>);

impl Answer {
    pub fn question(&self) -> &str {
        self.0.original()
    }

    pub fn text(&self) -> &str {
        self.0.answer()
    }

    /// The chunks the LLM saw, best first.
    pub fn contexts(&self) -> impl Iterator<Item = &str> {
        self.0.documents().iter().map(Document::content)
    }

    /// Distinct pages and sections the answer was based on, best first.
    pub fn sources(&self) -> Vec<Source<'_>> {
        let mut sources: Vec<Source<'_>> = Vec::with_capacity(self.0.documents().len());
        for source in self.0.documents().iter().map(Source::from) {
            if !sources.contains(&source) {
                sources.push(source);
            }
        }
        sources
    }
}

impl From<Query<states::Answered>> for Answer {
    fn from(query: Query<states::Answered>) -> Self {
        Self(query)
    }
}

/// Where a retrieved chunk came from.
#[derive(Debug, PartialEq, Eq)]
pub struct Source<'a> {
    pub page: &'a str,
    pub section: &'a str,
    pub origin: &'a str,
}

impl<'a> From<&'a Document> for Source<'a> {
    fn from(document: &'a Document) -> Self {
        let field = |key: &str| {
            document
                .metadata()
                .get(key)
                .and_then(|value| value.as_str())
                .unwrap_or_default()
        };

        Self {
            page: field(PAGE_TITLE),
            section: field(metadata_title::NAME),
            origin: field(PATH_METADATA),
        }
    }
}
