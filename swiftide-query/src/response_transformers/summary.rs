use std::sync::Arc;
use swiftide_core::{
    indexing::SimplePrompt,
    prelude::*,
    prompt::Prompt,
    querying::{states, Query},
    TransformResponse,
};

#[derive(Debug, Clone, Builder)]
pub struct Summary {
    #[builder(setter(custom))]
    client: Arc<dyn SimplePrompt>,
    #[builder(default = "default_prompt()")]
    prompt_template: Prompt,
}

impl Summary {
    pub fn builder() -> SummaryBuilder {
        SummaryBuilder::default()
    }

    /// Builds a new summary generator from a client that implements [`SimplePrompt`].
    ///
    /// Will try to summarize documents using an llm, instructed to preserve as much information as
    /// possible.
    ///
    /// # Panics
    ///
    /// Panics if the build failed
    pub fn from_client(client: impl SimplePrompt + 'static) -> Summary {
        SummaryBuilder::default()
            .client(client)
            .to_owned()
            .build()
            .expect("Failed to build Summary")
    }
}

impl SummaryBuilder {
    pub fn client(&mut self, client: impl SimplePrompt + 'static) -> &mut Self {
        self.client = Some(Arc::new(client) as Arc<dyn SimplePrompt>);
        self
    }
}

fn default_prompt() -> Prompt {
    indoc::indoc!(
        "
    Your job is to help a query tool find the right context.

    Summarize the following documents.

    ## Constraints
    * Do not add any information that is not available in the documents.
    * Summarize comprehensively and ensure no data that might be important is left out.
    * Summarize as a single markdown document

    ## Documents

    {% for document in documents -%}
    ---
    {{ document.content }}
    ---
    {% endfor -%}
    "
    )
    .into()
}

#[async_trait]
impl TransformResponse for Summary {
    #[tracing::instrument(skip_all)]
    async fn transform_response(
        &self,
        mut query: Query<states::Retrieved>,
    ) -> Result<Query<states::Retrieved>> {
        let new_response = self
            .client
            .prompt(
                self.prompt_template
                    .clone()
                    .with_context_value("documents", query.documents()),
            )
            .await?;
        query.transformed_response(new_response);

        Ok(query)
    }
}

#[cfg(test)]
mod test {
    use swiftide_core::document::Document;

    use super::*;

    assert_default_prompt_snapshot!("documents" => vec![Document::from("First document"), Document::from("Second Document")]);
}
