//! Generates questions and answers from a given text chunk and adds them as metadata.
use std::sync::Arc;

use crate::{ingestion::IngestionNode, SimplePrompt, Transformer};
use anyhow::Result;
use async_trait::async_trait;
use derive_builder::Builder;
use indoc::indoc;

/// This module defines the `MetadataQAText` struct and its associated methods,
/// which are used for generating metadata in the form of questions and answers
/// from a given text. It interacts with a client (e.g., `OpenAI`) to generate
/// these questions and answers based on the text chunk in an `IngestionNode`.

/// `MetadataQAText` is responsible for generating questions and answers
/// from a given text chunk. It uses a templated prompt to interact with a client
/// that implements the `SimplePrompt` trait.
#[derive(Debug, Clone, Builder)]
#[builder(setter(into, strip_option))]
pub struct MetadataQAText {
    #[builder(setter(custom))]
    client: Arc<dyn SimplePrompt>,
    #[builder(default = "default_prompt()")]
    prompt: String,
    #[builder(default = "5")]
    num_questions: usize,
    #[builder(default)]
    concurrency: Option<usize>,
}

impl MetadataQAText {
    pub fn builder() -> MetadataQATextBuilder {
        MetadataQATextBuilder::default()
    }

    pub fn from_client(client: impl SimplePrompt + 'static) -> MetadataQATextBuilder {
        MetadataQATextBuilder::default().client(client).to_owned()
    }
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
            concurrency: None,
        }
    }

    #[must_use]
    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = Some(concurrency);
        self
    }
}

/// Generates the default prompt template for generating questions and answers.
///
/// # Returns
///
/// A string containing the default prompt template.
fn default_prompt() -> String {
    indoc! {r"

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

        "}
    .to_string()
}

impl MetadataQATextBuilder {
    pub fn client(&mut self, client: impl SimplePrompt + 'static) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }
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

    fn concurrency(&self) -> Option<usize> {
        self.concurrency
    }
}

#[cfg(test)]
mod test {
    use crate::MockSimplePrompt;

    use super::*;

    #[tokio::test]
    async fn test_metadata_qacode() {
        let mut client = MockSimplePrompt::new();

        client
            .expect_prompt()
            .returning(|_| Ok("Q1: Hello\nA1: World".to_string()));

        let transformer = MetadataQAText::builder().client(client).build().unwrap();
        let node = IngestionNode::new("Some text");

        let result = transformer.transform_node(node).await.unwrap();

        assert_eq!(
            result.metadata.get("Questions and Answers").unwrap(),
            "Q1: Hello\nA1: World"
        );
    }
}
