//! Generate a summary and adds it as metadata
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use derive_builder::Builder;
use swiftide_core::{indexing::Node, prompt::PromptTemplate, SimplePrompt, Transformer};

pub const NAME: &str = "Summary";

/// This module defines the `MetadataSummary` struct and its associated methods,
/// which are used for generating metadata in the form of a summary
/// for a given text. It interacts with a client (e.g., `OpenAI`) to generate
/// the summary based on the text chunk in an `Node`.

/// `MetadataSummary` is responsible for generating a summary
/// for a given text chunk. It uses a templated prompt to interact with a client
/// that implements the `SimplePrompt` trait.
#[derive(Debug, Clone, Builder)]
#[builder(setter(into, strip_option))]
pub struct MetadataSummary {
    #[builder(setter(custom))]
    client: Arc<dyn SimplePrompt>,
    #[builder(default = "default_prompt()")]
    prompt_template: PromptTemplate,
    #[builder(default)]
    concurrency: Option<usize>,
}

impl MetadataSummary {
    pub fn builder() -> MetadataSummaryBuilder {
        MetadataSummaryBuilder::default()
    }

    pub fn from_client(client: impl SimplePrompt + 'static) -> MetadataSummaryBuilder {
        MetadataSummaryBuilder::default().client(client).to_owned()
    }
    /// Creates a new instance of `MetadataSummary`.
    ///
    /// # Arguments
    ///
    /// * `client` - An implementation of the `SimplePrompt` trait.
    ///
    /// # Returns
    ///
    /// A new instance of `MetadataSummary`.
    pub fn new(client: impl SimplePrompt + 'static) -> Self {
        Self {
            client: Arc::new(client),
            prompt_template: default_prompt(),
            concurrency: None,
        }
    }

    #[must_use]
    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = Some(concurrency);
        self
    }
}

/// Generates the default prompt template for extracting a summary.
fn default_prompt() -> PromptTemplate {
    include_str!("prompts/metadata_summary.prompt.md").into()
}

impl MetadataSummaryBuilder {
    pub fn client(&mut self, client: impl SimplePrompt + 'static) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }
}

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
        let prompt = self.prompt_template.to_prompt().with_node(&node);

        let response = self.client.prompt(prompt).await?;

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

        let prompt = template.to_prompt().with_node(&Node::new("test"));
        insta::assert_snapshot!(prompt.render().await.unwrap());
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
