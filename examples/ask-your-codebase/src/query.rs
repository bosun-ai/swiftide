//! Querying: answer a question from the indexed chunks and say where the answer came from.

use std::fmt;

use anyhow::{Result, bail};
use swiftide::{
    query::{
        self, Query, answers,
        query_transformers::{Embed, GenerateSubquestions},
        search_strategies::SimilaritySingleEmbedding,
        states,
    },
    traits::SimplePrompt,
};

use crate::{
    codebase::{Codebase, PATH_FIELD},
    embedding,
};

/// Answers `question` from the index of `codebase`, using the `top_k` most similar chunks.
pub async fn ask(
    codebase: &Codebase,
    question: &str,
    llm: impl SimplePrompt + Clone + 'static,
    top_k: u64,
) -> Result<Answer> {
    if !codebase.is_indexed() {
        bail!(
            "{} has not been indexed yet, run `ask-your-codebase index` first",
            codebase.root().display()
        );
    }

    // ANCHOR: query_pipeline
    let mut search: SimilaritySingleEmbedding = SimilaritySingleEmbedding::default();
    search.with_top_k(top_k);

    let pipeline = query::Pipeline::from_search_strategy(search)
        // Rephrase the question into several, to widen what the search can match
        .then_transform_query(
            GenerateSubquestions::builder()
                .client(llm.clone())
                .prompt_template(SUBQUESTIONS_PROMPT.into())
                .build()?,
        )
        // Embed with the same local model that embedded the chunks
        .then_transform_query(Embed::from_client(embedding::local_model()?))
        .then_retrieve(codebase.vector_store()?)
        // Answer the original question from the retrieved code, each chunk under its file path
        .then_answer(
            answers::Simple::builder()
                .client(llm)
                .prompt_template(ANSWER_PROMPT.into())
                .document_template(CHUNK_TEMPLATE)
                .build()?,
        );

    let answered = pipeline.query(question).await?;
    // ANCHOR_END: query_pipeline

    Ok(Answer::from(answered))
}

// ANCHOR: prompts
/// Widens the question into the follow-up questions its answer involves.
///
/// They are phrased like the questions generated for each chunk while indexing, so the two
/// embed close together.
const SUBQUESTIONS_PROMPT: &str = r"
Your job is to help a search find the code that answers a developer's question about a codebase.

Question: {{ question }}

Write {{ num_questions }} short follow-up questions that the code answering this question would also
answer. Think about the mechanisms involved: the main loop, the types and enum variants, the
conditions that are checked, the functions that are called, and the configuration that changes
the outcome.

## Constraints
* Phrase them the way a developer would ask about this codebase, e.g. 'When does the agent stop?'.
* Do not guess names of types or functions.
* Respond with the original question and the follow-up questions only, one per line, prefixed
  with '- '.
";

/// Renders a retrieved chunk under its file path, so the answer can cite it.
///
/// `metadata.path` is the `PATH_FIELD` metadata; a test keeps the two in step.
const CHUNK_TEMPLATE: &str = r"### {{ metadata.path }}

```
{{ content }}
```
";

/// Answers from the retrieved code and cites files.
const ANSWER_PROMPT: &str = r"
Answer a developer's question about a codebase, using snippets of its source code.

Question: {{ question }}

## Constraints
* Only use the snippets below. If they do not answer the question, say briefly what is missing.
* Be concrete: name the types, functions and variants involved and explain when they apply.
* Describe behaviour: what calls what, in which order, and under which conditions.
* Cite the file each statement comes from in backticks, e.g. (`src/lib.rs`).
* Use concise markdown. Add a short code example only if it shows how to use the API.

## Snippets

{{ documents }}
";
// ANCHOR_END: prompts

/// An answer and the source files of the chunks it was based on.
#[derive(Debug)]
pub struct Answer {
    text: String,
    sources: Vec<String>,
}

// ANCHOR: sources
impl From<Query<states::Answered>> for Answer {
    /// Keeps the distinct file paths of the retrieved chunks, most relevant first.
    fn from(query: Query<states::Answered>) -> Self {
        let mut sources: Vec<String> = Vec::new();

        for document in query.documents() {
            let Some(path) = document.metadata().get(PATH_FIELD).and_then(|v| v.as_str()) else {
                continue;
            };
            if !sources.iter().any(|source| source == path) {
                sources.push(path.to_owned());
            }
        }

        Self {
            text: query.answer().to_owned(),
            sources,
        }
    }
}
// ANCHOR_END: sources

impl fmt::Display for Answer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.text.trim())?;
        writeln!(f)?;
        writeln!(f, "Sources:")?;
        for source in &self.sources {
            writeln!(f, "  - {source}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use swiftide::{indexing::Metadata, prompt::Prompt, query::Document};

    use super::*;

    #[test]
    fn chunk_template_renders_the_file_path() {
        let document = Document::new(
            "fn main() {}",
            Some(Metadata::from((PATH_FIELD, "src/main.rs"))),
        );
        let document = serde_json::Value::from(&document);

        let rendered = Prompt::from(CHUNK_TEMPLATE)
            .with_context_value("metadata", document["metadata"].clone())
            .with_context_value("content", document["content"].clone())
            .render()
            .unwrap();

        assert!(rendered.starts_with("### src/main.rs\n"));
        assert!(rendered.contains("fn main() {}"));
    }
}
