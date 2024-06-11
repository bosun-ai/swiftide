use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use indoc::indoc;
use infrastructure::SimplePrompt;

use crate::{ingestion_node::IngestionNode, traits::Transformer};

#[derive(Debug)]
pub struct MetadataQACode {
    client: Arc<dyn SimplePrompt>,
    prompt: String,
    num_questions: usize,
}

impl MetadataQACode {
    pub fn new(client: impl SimplePrompt + 'static) -> Self {
        Self {
            client: Arc::new(client),
            prompt: default_prompt(),
            num_questions: 5,
        }
    }
}

fn default_prompt() -> String {
    indoc! {r#"

            # Task
            Your task is to generate questions and answers for the given code. 

            Given that somebody else might ask questions about the code, consider things like:
            * What does this code do?
            * What other internal parts does the code use?
            * Does this code have any dependencies?
            * What are some potential use cases for this code?
            * ... and so on

            # Constraints 
            * Generate only {questions} questions and answers.
            * Only respond in the example format
            * Only respond with questions and answers that can be derived from the code.

            # Example
            Respond in the following example format and do not include anything else:

            ```
            Q1: What does this code do?
            A1: It transforms strings into integers.
            Q2: What other internal parts does the code use?
            A2: A hasher to hash the strings.
            ```

            # Code
            ```
            {code}
            ```

        "#}
    .to_string()
}

#[async_trait]
impl Transformer for MetadataQACode {
    #[tracing::instrument(skip_all, name = "transformers.metadata_qa_code")]
    async fn transform_node(&self, mut node: IngestionNode) -> Result<IngestionNode> {
        let prompt = self
            .prompt
            .replace("{questions}", &self.num_questions.to_string())
            .replace("{code}", &node.chunk);

        let response = self
            .client
            .prompt(&prompt, infrastructure::DEFAULT_OPENAI_MODEL)
            .await?;

        node.metadata
            .insert("Questions and Answers".to_string(), response);

        Ok(node)
    }
}
