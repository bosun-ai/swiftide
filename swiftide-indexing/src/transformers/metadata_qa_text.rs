//! Generates questions and answers from a given text chunk and adds them as metadata.
//! This module defines the `MetadataQAText` struct and its associated methods,
//! which are used for generating metadata in the form of questions and answers
//! from a given text. It interacts with a client (e.g., `OpenAI`) to generate
//! these questions and answers based on the text chunk in an `Node`.

use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::{Transformer, indexing::Node};

/// `MetadataQAText` is responsible for generating questions and answers
/// from a given text chunk. It uses a templated prompt to interact with a client
/// that implements the `SimplePrompt` trait.
#[swiftide_macros::indexing_transformer(
    metadata_field_name = "Questions and Answers (text)",
    default_prompt_file = "prompts/metadata_qa_text.prompt.md"
)]
pub struct MetadataQAText {
    #[builder(default = "5")]
    num_questions: usize,
}

#[async_trait]
impl Transformer for MetadataQAText {
    /// Transforms an `Node` by generating questions and answers
    /// based on the text chunk within the node.
    ///
    /// # Arguments
    ///
    /// * `node` - The `Node` containing the text chunk to process.
    ///
    /// # Returns
    ///
    /// A `Result` containing the transformed `Node` with added metadata,
    /// or an error if the transformation fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if the client fails to generate
    /// questions and answers from the provided prompt.
    #[tracing::instrument(skip_all, name = "transformers.metadata_qa_text")]
    async fn transform_node(&self, mut node: Node) -> Result<Node> {
        let prompt = self
            .prompt_template
            .clone()
            .with_node(&node)
            .with_context_value("questions", self.num_questions);

        let response = self.prompt(prompt).await?;

        node.metadata.insert(NAME, response);

        Ok(node)
    }

    fn concurrency(&self) -> Option<usize> {
        self.concurrency
    }
}

#[cfg(test)]
mod test {
    use swiftide_core::MockSimplePrompt;

    use super::*;

    #[tokio::test]
    async fn test_template() {
        let template = default_prompt();

        let prompt = template
            .clone()
            .with_node(&Node::new("test"))
            .with_context_value("questions", 5);
        insta::assert_snapshot!(prompt.render().unwrap());
    }

    #[tokio::test]
    async fn test_metadata_qacode() {
        let mut client = MockSimplePrompt::new();

        client
            .expect_prompt()
            .returning(|_| Ok("Q1: Hello\nA1: World".to_string()));

        let transformer = MetadataQAText::builder().client(client).build().unwrap();
        let node = Node::new("Some text");

        let result = transformer.transform_node(node).await.unwrap();

        assert_eq!(
            result.metadata.get("Questions and Answers (text)").unwrap(),
            "Q1: Hello\nA1: World"
        );
    }
}
