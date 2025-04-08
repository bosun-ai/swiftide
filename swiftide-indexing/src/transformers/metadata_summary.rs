//! Generate a summary and adds it as metadata
//! This module defines the `MetadataSummary` struct and its associated methods,
//! which are used for generating metadata in the form of a summary
//! for a given text. It interacts with a client (e.g., `OpenAI`) to generate
//! the summary based on the text chunk in an `Node`.

use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::{indexing::Node, Transformer};

/// `MetadataSummary` is responsible for generating a summary
/// for a given text chunk. It uses a templated prompt to interact with a client
/// that implements the `SimplePrompt` trait.
#[swiftide_macros::indexing_transformer(
    metadata_field_name = "Summary",
    default_prompt_file = "prompts/metadata_summary.prompt.md"
)]
pub struct MetadataSummary {}

#[async_trait]
impl Transformer for MetadataSummary {
    /// Transforms an `Node` by extracting a summary
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
    /// a summary from the provided prompt.
    #[tracing::instrument(skip_all, name = "transformers.metadata_summary")]
    async fn transform_node(&self, mut node: Node) -> Result<Node> {
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

    #[tokio::test]
    async fn test_template() {
        let template = default_prompt();

        let prompt = template.clone().with_node(&Node::new("test"));
        insta::assert_snapshot!(prompt.render().unwrap());
    }

    #[tokio::test]
    async fn test_metadata_summary() {
        let mut client = MockSimplePrompt::new();

        client
            .expect_prompt()
            .returning(|_| Ok("A Summary".to_string()));

        let transformer = MetadataSummary::builder().client(client).build().unwrap();
        let node = Node::new("Some text");

        let result = transformer.transform_node(node).await.unwrap();

        assert_eq!(result.metadata.get("Summary").unwrap(), "A Summary");
    }
}
