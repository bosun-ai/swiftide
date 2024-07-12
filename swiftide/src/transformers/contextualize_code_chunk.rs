//! Generate questions and answers based on code chunks and add them as metadata
use derive_builder::Builder;
use std::sync::Arc;

use crate::{indexing::Node, SimplePrompt, Transformer};
use anyhow::Result;
use async_trait::async_trait;
use indoc::indoc;

/// `ContextualizeCodeChunk` adds context to code chunks by making use of file-level metadata.
#[derive(Debug, Clone, Builder)]
#[builder(setter(into, strip_option))]
pub struct ContextualizeCodeChunk {
    #[builder(setter(custom))]
    client: Arc<dyn SimplePrompt>,
    #[builder(default = "default_prompt()")]
    prompt: String,
    #[builder(default)]
    concurrency: Option<usize>,
}

impl ContextualizeCodeChunk {
    pub fn builder() -> ContextualizeCodeChunkBuilder {
        ContextualizeCodeChunkBuilder::default()
    }

    pub fn from_client(client: impl SimplePrompt + 'static) -> ContextualizeCodeChunkBuilder {
        ContextualizeCodeChunkBuilder::default()
            .client(client)
            .to_owned()
    }
    /// Creates a new instance of `ContextualizeCodeChunk`.
    ///
    /// # Arguments
    ///
    /// * `client` - An implementation of the `SimplePrompt` trait used to generate questions and answers.
    ///
    /// # Returns
    ///
    /// A new instance of `ContextualizeCodeChunk` with a default prompt and a default number of questions.
    pub fn new(client: impl SimplePrompt + 'static) -> Self {
        Self {
            client: Arc::new(client),
            prompt: default_prompt(),
            concurrency: None,
        }
    }

    #[must_use]
    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = Some(concurrency);
        self
    }
}

/// Returns the default prompt template for generating questions and answers.
///
/// This template includes placeholders for the number of questions and the code chunk.
///
/// # Returns
///
/// A string representing the default prompt template.
fn default_prompt() -> String {
    indoc! {r"

            # Task
            Your task is to filter the given file context to the code chunk provided. The goal is
            to provide a context that is still contains the lines needed for understanding the code in the chunk whilst
            leaving out any irrelevant information.

            # Constraints
            * Only use lines from the provided context, do not add any additional information
            * Ensure that the selection you make is the most appropriate for the code chunk
            * You do not need to repeat the code chunk in your response, it will be appended directly
              after your response.

            # Code
            ```
            {code}
            ```

            # Context
            ```
            {context}
            ```
        "}
    .to_string()
}

impl ContextualizeCodeChunkBuilder {
    pub fn client(&mut self, client: impl SimplePrompt + 'static) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }
}

#[async_trait]
impl Transformer for ContextualizeCodeChunk {
    /// Asynchronously transforms an `Node` by adding context to it based on the original file metadata.
    ///
    /// This method uses the `SimplePrompt` client to merge the chunk with its context.
    ///
    /// # Arguments
    ///
    /// * `node` - The `Node` to be transformed.
    ///
    /// # Returns
    ///
    /// A result containing the transformed `Node` or an error if the transformation fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if the `SimplePrompt` client fails to generate a response.
    #[tracing::instrument(skip_all, name = "transformers.metadata_qa_code")]
    async fn transform_node(&self, mut node: Node) -> Result<Node> {
        let needs_context = match node.metadata.get("Original Size") {
            Some(size) => size.parse::<usize>().unwrap() > node.chunk.len(),
            None => false,
        };

        let metadata = &mut node.metadata;
        let maybe_context = metadata.get("Context (code)");
        let has_context = maybe_context.is_some();
        let context;
        if !needs_context || !has_context {
            return Ok(node);
        } else {
            context = maybe_context.unwrap().clone();
        }

        let prompt = self
            .prompt
            .replace("{context}", context.as_str())
            .replace("{code}", &node.chunk);

        let response = self.client.prompt(&prompt).await?;

        node.chunk = response + "\n\n" + &node.chunk;

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
    async fn test_contextualize_code_chunk() {
        let mut client = MockSimplePrompt::new();

        client
            .expect_prompt()
            .returning(|_| Ok("RelevantContext".to_string()));

        let transformer = ContextualizeCodeChunk::builder()
            .client(client)
            .build()
            .unwrap();
        let mut node = Node::new("Some text");
        node.metadata
            .insert("Original Size".to_string(), "100".to_string());
        node.metadata
            .insert("Context (code)".to_string(), "Some context".to_string());

        let result = transformer.transform_node(node).await.unwrap();

        assert_eq!(result.chunk, "RelevantContext\n\nSome text");
    }
}
