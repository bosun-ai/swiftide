//! Generate an answer based on the current query
//!
//! For example, after retrieving documents, and those are summarized,
//! will answer the original question with the current text in the query.
//!
//! WARN: If no previous response transformations have been done, the last query before retrieval
//! will be used.
use std::sync::Arc;
use swiftide_core::{
    indexing::SimplePrompt,
    prelude::*,
    prompt::PromptTemplate,
    querying::{states, Query},
    Answer,
};

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
    indoc::indoc!(
        "
    Answer the following question based on the context provided:
    {{ question }}

    ## Constraints
    * Do not include any information that is not in the provided context.
    * If the question cannot be answered by the provided context, state that it cannot be answered.
    * Answer the question completely and format it as markdown.

    ## Context

    {{ context }}
    "
    )
    .into()
}

#[async_trait]
impl Answer for Simple {
    #[tracing::instrument]
    async fn answer(&self, query: Query<states::Retrieved>) -> Result<Query<states::Answered>> {
        let answer = self
            .client
            .prompt(
                self.prompt_template
                    .to_prompt()
                    .with_context_value("question", query.original())
                    .with_context_value("current", query.current()),
            )
            .await?;

        Ok(query.answered(answer))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    assert_default_prompt_snapshot!("question" => "What is love?", "documents" => vec!["First document", "Second Document"]);
}
