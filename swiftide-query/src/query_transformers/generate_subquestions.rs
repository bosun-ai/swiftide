use anyhow::Result;
use std::sync::Arc;

use async_trait::async_trait;
use derive_builder::Builder;
use swiftide::{prompt::PromptTemplate, SimplePrompt};

use crate::{
    query::{states, Query},
    traits::TransformQuery,
};

#[derive(Debug, Clone, Builder)]
pub struct GenerateSubquestions {
    #[builder(setter(custom))]
    client: Arc<dyn SimplePrompt>,
    #[builder(default = "default_prompt()")]
    prompt_template: PromptTemplate,
    #[builder(default = "5")]
    num_questions: usize,
}

impl GenerateSubquestions {
    pub fn builder() -> GenerateSubquestionsBuilder {
        GenerateSubquestionsBuilder::default()
    }

    pub fn from_client(client: impl SimplePrompt + 'static) -> GenerateSubquestionsBuilder {
        GenerateSubquestionsBuilder::default()
            .client(client)
            .to_owned()
    }
}

impl GenerateSubquestionsBuilder {
    pub fn client(&mut self, client: impl SimplePrompt + 'static) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }
}

fn default_prompt() -> PromptTemplate {
    r"
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
    ".into()
}

#[async_trait]
impl TransformQuery for GenerateSubquestions {
    async fn transform_query(
        &self,
        mut query: Query<states::Pending>,
    ) -> Result<Query<states::Pending>> {
        let client = Arc::clone(&self.client);
        let new_query = client
            .prompt(
                self.prompt_template
                    .to_prompt()
                    .with_context_value("question", query.current())
                    .with_context_value("num_questions", self.num_questions),
            )
            .await?;
        query.update(new_query);

        Ok(query)
    }
}
