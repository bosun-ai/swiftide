use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use indoc::indoc;
use infrastructure::SimplePrompt;

use crate::{ingestion_node::IngestionNode, traits::Transformer};

#[derive(Debug)]
pub struct MetadataQAText {
    client: Arc<dyn SimplePrompt>,
    prompt: String,
    num_questions: usize,
}

impl MetadataQAText {
    pub fn new(client: Arc<dyn SimplePrompt>) -> Self {
        Self {
            client,
            prompt: default_prompt(),
            num_questions: 5,
        }
    }
}

fn default_prompt() -> String {
    indoc! {r#"

            # Task
            Your task is to generate questions and answers for the given text. 

            Given that somebody else might ask questions about the text, consider things like:
            * What does this text do?
            * What other internal parts does the text use?
            * Does this text have any dependencies?
            * What are some potential use cases for this text?
            * ... and so on

            # Constraints 
            * Generate at most {questions} questions and answers.
            * Only respond in the example format
            * Only respond with questions and answers that can be derived from the text.

            # Example
            Respond in the following example format and do not include anything else:

            ```
            Q1: What is the capital of France?
            A1: Paris.
            ```

            # text
            ```
            {text}
            ```

        "#}
    .to_string()
}

#[async_trait]
impl Transformer for MetadataQAText {
    #[tracing::instrument(skip_all, name = "transformers.metadata_qa_text")]
    async fn transform_node(&self, mut node: IngestionNode) -> Result<IngestionNode> {
        let prompt = self
            .prompt
            .replace("{questions}", &self.num_questions.to_string())
            .replace("{text}", &node.chunk);

        let response = self
            .client
            .prompt(&prompt, infrastructure::DEFAULT_OPENAI_MODEL)
            .await?;

        node.metadata
            .insert("Questions and Answers".to_string(), response);

        Ok(node)
    }
}
