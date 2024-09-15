//! Generate an answer based on the current query
//!
//! For example, after retrieving documents, and those are summarized,
//! will answer the original question with the current text in the query.
//!
//! If `current` on the Query is empty, it will concatenate the documents
//! as context instead.
use std::sync::Arc;
use swiftide_core::{
    indexing::SimplePrompt,
    prelude::*,
    prompt::PromptTemplate,
    querying::{states, Query},
    Answer,
};

/// Generate an answer based on the current query
///
/// For example, after retrieving documents, and those are summarized,
/// will answer the original question with the current text in the query.
///
/// If `current` on the Query is empty, it will concatenate the documents
/// as context instead.
#[derive(Debug, Clone, Builder)]
pub struct Simple {
    #[builder(setter(custom))]
    client: Arc<dyn SimplePrompt>,
    #[builder(default = "default_prompt()")]
    prompt_template: PromptTemplate,
}

impl Simple {
    pub fn builder() -> SimpleBuilder {
        SimpleBuilder::default()
    }

    /// Builds a new simple answer generator from a client that implements [`SimplePrompt`].
    ///
    /// # Panics
    ///
    /// Panics if the build failed
    pub fn from_client(client: impl SimplePrompt + 'static) -> Simple {
        SimpleBuilder::default()
            .client(client)
            .to_owned()
            .build()
            .expect("Failed to build Simple")
    }
}

impl SimpleBuilder {
    pub fn client(&mut self, client: impl SimplePrompt + 'static) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }
}

fn default_prompt() -> PromptTemplate {
    indoc::indoc! {"
    Answer the following question based on the context provided:
    {{ question }}

    ## Constraints
    * Do not include any information that is not in the provided context.
    * If the question cannot be answered by the provided context, state that it cannot be answered.
    * Answer the question completely and format it as markdown.

    ## Context

    {{ context }}
    "}
    .into()
}

#[async_trait]
impl Answer for Simple {
    #[tracing::instrument(skip_self)]
    async fn answer(&self, query: Query<states::Retrieved>) -> Result<Query<states::Answered>> {
        let context = if query.current().is_empty() {
            &query.documents().join("\n---\n")
        } else {
            query.current()
        };

        let answer = self
            .client
            .prompt(
                self.prompt_template
                    .to_prompt()
                    .with_context_value("question", query.original())
                    .with_context_value("context", context),
            )
            .await?;

        Ok(query.answered(answer))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    assert_default_prompt_snapshot!("question" => "What is love?", "context" => "My context");
}
