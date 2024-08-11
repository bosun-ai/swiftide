//! Generate questions and answers based on code chunks and add them as metadata
use derive_builder::Builder;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::{
    indexing::{IndexingDefaults, Node},
    prompt::PromptTemplate,
    SimplePrompt, Transformer,
};

/// `MetadataQACode` is responsible for generating questions and answers based on code chunks.
/// This struct integrates with the indexing pipeline to enhance the metadata of each code chunk
/// by adding relevant questions and answers.
#[swiftide_macros::indexing_transformer(
    metadata_field_name = "Questions and Answers (code)",
    default_prompt_file = "prompts/metadata_qa_code.prompt.md"
)]
pub struct MetadataQACode {
    #[builder(default = "5")]
    num_questions: usize,
}

#[async_trait]
impl Transformer for MetadataQACode {
    /// Asynchronously transforms a `Node` by generating questions and answers for its code chunk.
    ///
    /// This method uses the `SimplePrompt` client to generate questions and answers based on the code chunk
    /// and adds this information to the node's metadata.
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
        let mut prompt = self
            .prompt_template
            .to_prompt()
            .with_node(&node)
            .with_context_value("questions", self.num_questions);

        if let Some(outline) = node.metadata.get("Outline") {
            prompt = prompt.with_context_value("outline", outline.as_str());
        }

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
            .to_prompt()
            .with_node(&Node::new("test"))
            .with_context_value("questions", 5);
        insta::assert_snapshot!(prompt.render().await.unwrap());
    }

    #[tokio::test]
    async fn test_template_with_outline() {
        let template = default_prompt();

        let prompt = template
            .to_prompt()
            .with_node(&Node::new("test"))
            .with_context_value("questions", 5)
            .with_context_value("outline", "Test outline");
        insta::assert_snapshot!(prompt.render().await.unwrap());
    }

    #[tokio::test]
    async fn test_metadata_qacode() {
        let mut client = MockSimplePrompt::new();

        client
            .expect_prompt()
            .returning(|_| Ok("Q1: Hello\nA1: World".to_string()));

        let transformer = MetadataQACode::builder().client(client).build().unwrap();
        let node = Node::new("Some text");

        let result = transformer.transform_node(node).await.unwrap();

        assert_eq!(
            result.metadata.get("Questions and Answers (code)").unwrap(),
            "Q1: Hello\nA1: World"
        );
    }
}
