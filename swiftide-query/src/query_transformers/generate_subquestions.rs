//! Generate subquestions for a query
//!
//! Useful for similarity search where you want a wider vector coverage
use std::sync::Arc;
use swiftide_core::{
    indexing::SimplePrompt,
    prelude::*,
    prompt::Prompt,
    querying::{states, Query, TransformQuery},
};

#[derive(Debug, Clone, Builder)]
pub struct GenerateSubquestions {
    #[builder(setter(custom))]
    client: Arc<dyn SimplePrompt>,
    #[builder(default = "default_prompt()")]
    prompt_template: Prompt,
    #[builder(default = "5")]
    num_questions: usize,
}

impl GenerateSubquestions {
    pub fn builder() -> GenerateSubquestionsBuilder {
        GenerateSubquestionsBuilder::default()
    }

    /// Builds a new subquestions generator from a client that implements [`SimplePrompt`]
    ///
    /// # Panics
    ///
    /// Panics if the build failed
    pub fn from_client(client: impl SimplePrompt + 'static) -> GenerateSubquestions {
        GenerateSubquestionsBuilder::default()
            .client(client)
            .to_owned()
            .build()
            .expect("Failed to build GenerateSubquestions")
    }
}

impl GenerateSubquestionsBuilder {
    pub fn client(&mut self, client: impl SimplePrompt + 'static) -> &mut Self {
        self.client = Some(Arc::new(client) as Arc<dyn SimplePrompt>);
        self
    }
}

fn default_prompt() -> Prompt {
    indoc::indoc!("
    Your job is to help a query tool find the right context.

    Given the following question:
    {{question}}

    Please think of {{num_questions}}  additional questions that can help answering the original question.

    Especially consider what might be relevant to answer the question, like dependencies, usage and structure of the code.

    Please respond with the original question and the additional questions only.

    ## Example

    - {{question}}
    - Additional question 1
    - Additional question 2
    - Additional question 3
    - Additional question 4
    - Additional question 5
    ").into()
}

#[async_trait]
impl TransformQuery for GenerateSubquestions {
    #[tracing::instrument(skip_self)]
    async fn transform_query(
        &self,
        mut query: Query<states::Pending>,
    ) -> Result<Query<states::Pending>> {
        let new_query = self
            .client
            .prompt(
                self.prompt_template
                    .clone()
                    .with_context_value("question", query.current())
                    .with_context_value("num_questions", self.num_questions),
            )
            .await?;
        query.transformed_query(new_query);

        Ok(query)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    assert_default_prompt_snapshot!("question" => "What is love?", "num_questions" => 5);
}
