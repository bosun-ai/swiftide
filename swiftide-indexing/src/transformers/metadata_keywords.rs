//! Extract keywords from a node and add them as metadata
//! This module defines the `MetadataKeywords` struct and its associated methods,
//! which are used for generating metadata in the form of keywords
//! for a given text. It interacts with a client (e.g., `OpenAI`) to generate
//! the keywords based on the text chunk in a `TextNode`.
use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::{Transformer, indexing::TextNode};

/// `MetadataKeywords` is responsible for generating keywords
/// for a given text chunk. It uses a templated prompt to interact with a client
/// that implements the `SimplePrompt` trait.
#[swiftide_macros::indexing_transformer(
    default_prompt_file = "prompts/metadata_keywords.prompt.md",
    metadata_field_name = "Keywords"
)]
pub struct MetadataKeywords {}

#[async_trait]
impl Transformer for MetadataKeywords {
    type Input = String;
    type Output = String;

    /// Transforms an `TextNode` by extracting a keywords
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
    /// a keywords from the provided prompt.
    #[tracing::instrument(skip_all, name = "transformers.metadata_keywords")]
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
    async fn test_metadata_keywords() {
        let mut client = MockSimplePrompt::new();

        client
            .expect_prompt()
            .returning(|_| Ok("important,keywords".to_string()));

        let transformer = MetadataKeywords::builder().client(client).build().unwrap();
        let node = TextNode::new("Some text");

        let result = transformer.transform_node(node).await.unwrap();

        assert_eq!(
            result.metadata.get("Keywords").unwrap(),
            "important,keywords"
        );
    }
}
