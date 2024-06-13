use std::sync::Arc;

use crate::{ingestion::IngestionNode, SimplePrompt, Transformer};
use anyhow::Result;
use async_trait::async_trait;
use indoc::indoc;

/// This module defines the `MetadataQAText` struct and its associated methods,
/// which are used for generating metadata in the form of questions and answers
/// from a given text. It interacts with a client (e.g., OpenAI) to generate
/// these questions and answers based on the text chunk in an `IngestionNode`.

/// `MetadataQAText` is responsible for generating questions and answers
/// from a given text chunk. It uses a templated prompt to interact with a client
/// that implements the `SimplePrompt` trait.
#[derive(Debug)]
pub struct MetadataQAText {
    client: Arc<dyn SimplePrompt>,
    prompt: String,
    num_questions: usize,
}

impl MetadataQAText {
    /// Creates a new instance of `MetadataQAText`.
    ///
    /// # Arguments
    ///
    /// * `client` - An implementation of the `SimplePrompt` trait.
    ///
    /// # Returns
    ///
    /// A new instance of `MetadataQAText`.
    pub fn new(client: impl SimplePrompt + 'static) -> Self {
        Self {
            client: Arc::new(client),
            prompt: default_prompt(),
            num_questions: 5,
        }
    }
}

/// Generates the default prompt template for generating questions and answers.
///
/// # Returns
///
/// A string containing the default prompt template.
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
    /// Transforms an `IngestionNode` by generating questions and answers
    /// based on the text chunk within the node.
    ///
    /// # Arguments
    ///
    /// * `node` - The `IngestionNode` containing the text chunk to process.
    ///
    /// # Returns
    ///
    /// A `Result` containing the transformed `IngestionNode` with added metadata,
    /// or an error if the transformation fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if the client fails to generate
    /// questions and answers from the provided prompt.
    #[tracing::instrument(skip_all, name = "transformers.metadata_qa_text")]
    async fn transform_node(&self, mut node: IngestionNode) -> Result<IngestionNode> {
        let prompt = self
            .prompt
            .replace("{questions}", &self.num_questions.to_string())
            .replace("{text}", &node.chunk);

        let response = self.client.prompt(&prompt).await?;

        node.metadata
            .insert("Questions and Answers".to_string(), response);

        Ok(node)
    }
}
