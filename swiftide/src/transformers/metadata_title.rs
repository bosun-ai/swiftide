use std::sync::Arc;

use crate::{ingestion::IngestionNode, SimplePrompt, Transformer};
use anyhow::Result;
use async_trait::async_trait;
use derive_builder::Builder;
use indoc::indoc;

/// This module defines the `MetadataTitle` struct and its associated methods,
/// which are used for generating metadata in the form of a title
/// for a given text. It interacts with a client (e.g., OpenAI) to generate
/// these questions and answers based on the text chunk in an `IngestionNode`.

/// `MetadataTitle` is responsible for generating a title
/// for a given text chunk. It uses a templated prompt to interact with a client
/// that implements the `SimplePrompt` trait.
#[derive(Debug, Clone, Builder)]
#[builder(setter(into, strip_option))]
pub struct MetadataTitle {
    #[builder(setter(custom))]
    client: Arc<dyn SimplePrompt>,
    #[builder(default = "default_prompt()")]
    prompt: String,
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
            prompt: default_prompt(),
            concurrency: None,
        }
    }

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
    indoc! {r#"

            # Task
            Your task is to generate a descriptive, concise title for the given text

            # Constraints 
            * Only respond in the example format
            * Respond with a title that is accurate and descriptive without fluff

            # Example
            Respond in the following example format and do not include anything else:

            ```
            <title>
            ```

            # Text
            ```
            {text}
            ```

        "#}
    .to_string()
}

impl MetadataTitleBuilder {
    pub fn client(&mut self, client: impl SimplePrompt + 'static) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }
}

#[async_trait]
impl Transformer for MetadataTitle {
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
    #[tracing::instrument(skip_all, name = "transformers.metadata_title")]
    async fn transform_node(&self, mut node: IngestionNode) -> Result<IngestionNode> {
        let prompt = self.prompt.replace("{text}", &node.chunk);

        let response = self.client.prompt(&prompt).await?;

        node.metadata.insert("Title".to_string(), response);

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
    async fn test_metadata_title() {
        let mut client = MockSimplePrompt::new();

        client
            .expect_prompt()
            .returning(|_| Ok("A Title".to_string()));

        let transformer = MetadataTitle::builder().client(client).build().unwrap();
        let node = IngestionNode::new("Some text");

        let result = transformer.transform_node(node).await.unwrap();

        assert_eq!(result.metadata.get("Title").unwrap(), "A Title");
    }
}
