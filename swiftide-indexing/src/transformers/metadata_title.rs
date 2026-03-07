//! Generate a title and adds it as metadata
//! This module defines the `MetadataTitle` struct and its associated methods,
//! which are used for generating metadata in the form of a title
//! for a given text. It interacts with a client (e.g., `OpenAI`) to generate
//! these questions and answers based on the text chunk in an `TextNode`.
use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::{Transformer, indexing::TextNode};

/// `MetadataTitle` is responsible for generating a title
/// for a given text chunk. It uses a templated prompt to interact with a client
/// that implements the `SimplePrompt` trait.
#[swiftide_macros::indexing_transformer(
    metadata_field_name = "Title",
    default_prompt_file = "prompts/metadata_title.prompt.md"
)]
pub struct MetadataTitle {}

#[async_trait]
impl Transformer for MetadataTitle {
    type Input = String;
    type Output = String;
    /// Transforms an `TextNode` by generating questions and answers
    /// based on the text chunk within the node.
    ///
    /// # Arguments
    ///
    /// * `node` - The `TextNode` containing the text chunk to process.
    ///
    /// # Returns
    ///
    /// A `Result` containing the transformed `TextNode` with added metadata,
    /// or an error if the transformation fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if the client fails to generate
    /// questions and answers from the provided prompt.
    #[tracing::instrument(skip_all, name = "transformers.metadata_title")]
    async fn transform_node(&self, mut node: TextNode) -> Result<TextNode> {
        let prompt = self.prompt_template.clone().with_node(&node);

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

    #[test_log::test(tokio::test)]
    async fn test_template() {
        let template = default_prompt();

        let prompt = template.clone().with_node(&TextNode::new("test"));
        insta::assert_snapshot!(prompt.render().unwrap());
    }

    #[tokio::test]
    async fn test_metadata_title() {
        let mut client = MockSimplePrompt::new();

        client
            .expect_prompt()
            .returning(|_| Ok("A Title".to_string()));

        let transformer = MetadataTitle::builder().client(client).build().unwrap();
        let node = TextNode::new("Some text");

        let result = transformer.transform_node(node).await.unwrap();

        assert_eq!(result.metadata.get("Title").unwrap(), "A Title");
    }
}
