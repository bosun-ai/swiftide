//! Generate a title and adds it as metadata
use std::sync::Arc;

use crate::{indexing::Node, prompt::PromptTemplate, SimplePrompt, Transformer};
use anyhow::Result;
use async_trait::async_trait;
use derive_builder::Builder;

pub const NAME: &str = "Title";

/// This module defines the `MetadataTitle` struct and its associated methods,
/// which are used for generating metadata in the form of a title
/// for a given text. It interacts with a client (e.g., `OpenAI`) to generate
/// these questions and answers based on the text chunk in an `Node`.

/// `MetadataTitle` is responsible for generating a title
/// for a given text chunk. It uses a templated prompt to interact with a client
/// that implements the `SimplePrompt` trait.
#[derive(Debug, Clone, Builder)]
#[builder(setter(into, strip_option))]
pub struct MetadataTitle {
    #[builder(setter(custom))]
    client: Arc<dyn SimplePrompt>,
    #[builder(default = "default_prompt()")]
    prompt_template: PromptTemplate,
    #[builder(default)]
    concurrency: Option<usize>,
}

impl MetadataTitle {
    pub fn builder() -> MetadataTitleBuilder {
        MetadataTitleBuilder::default()
    }

    pub fn from_client(client: impl SimplePrompt + 'static) -> MetadataTitleBuilder {
        MetadataTitleBuilder::default().client(client).to_owned()
    }
    /// Creates a new instance of `MetadataTitle`.
    ///
    /// # Arguments
    ///
    /// * `client` - An implementation of the `SimplePrompt` trait.
    ///
    /// # Returns
    ///
    /// A new instance of `MetadataTitle`.
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

/// Generates the default prompt template for generating questions and answers.
fn default_prompt() -> PromptTemplate {
    PromptTemplate::from_compiled_template_name("src/transformers/prompts/metadata_title.prompt.md")
}

impl MetadataTitleBuilder {
    pub fn client(&mut self, client: impl SimplePrompt + 'static) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }
}

#[async_trait]
impl Transformer for MetadataTitle {
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
    #[tracing::instrument(skip_all, name = "transformers.metadata_title")]
    async fn transform_node(&self, mut node: Node) -> Result<Node> {
        let prompt = self.prompt_template.to_prompt().with_node(&node);

        let response = self.client.prompt(prompt).await?;

        node.metadata.insert(NAME.into(), response);

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
    async fn test_template() {
        let template = default_prompt();

        let prompt = template.to_prompt().with_node(&Node::new("test"));
        insta::assert_snapshot!(prompt.render().await.unwrap());
    }

    #[tokio::test]
    async fn test_metadata_title() {
        let mut client = MockSimplePrompt::new();

        client
            .expect_prompt()
            .returning(|_| Ok("A Title".to_string()));

        let transformer = MetadataTitle::builder().client(client).build().unwrap();
        let node = Node::new("Some text");

        let result = transformer.transform_node(node).await.unwrap();

        assert_eq!(result.metadata.get("Title").unwrap(), "A Title");
    }
}
