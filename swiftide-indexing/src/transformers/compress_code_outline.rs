//! `CompressCodeOutline` is a transformer that reduces the size of the outline of a the parent file of a chunk to make it more relevant to the chunk.
use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::{
    indexing::{IndexingDefaults, Node},
    Transformer,
};

/// `CompressCodeChunk` rewrites the "Outline" metadata field of a chunk to
/// condense it and make it more relevant to the chunk in question. It is useful as a
/// step after chunking a file that has had outline generated for it with `FileToOutlineTreeSitter`.
#[swiftide_macros::indexing_transformer(
    metadata_field_name = "Code Outline",
    default_prompt_file = "prompts/compress_code_outline.prompt.md"
)]
pub struct CompressCodeOutline {}

fn extract_markdown_codeblock(text: String) -> String {
    let re = regex::Regex::new(r"(?sm)```\w*\n(.*?)```").unwrap();
    let captures = re.captures(text.as_str());
    captures
        .map(|c| c.get(1).unwrap().as_str().to_string())
        .unwrap_or(text)
}

#[async_trait]
impl Transformer for CompressCodeOutline {
    /// Asynchronously transforms an `Node` by reducing the size of the outline to make it more relevant to the chunk.
    ///
    /// This method uses the `SimplePrompt` client to compress the outline of the `Node` and updates the `Node` with the compressed outline.
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
    #[tracing::instrument(skip_all, name = "transformers.compress_code_outline")]
    async fn transform_node(&self, mut node: Node) -> Result<Node> {
        let maybe_outline = node.metadata.get("Outline");

        let Some(outline) = maybe_outline else {
            return Ok(node);
        };

        let prompt = self
            .prompt_template
            .to_prompt()
            .with_context_value("outline", outline.as_str())
            .with_context_value("code", node.chunk.as_str());

        let response = extract_markdown_codeblock(self.prompt(prompt).await?);

        node.metadata.insert("Outline".to_string(), response);

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
    async fn test_compress_code_template() {
        let template = default_prompt();

        let outline = "Relevant Outline";
        let code = "Code using outline";

        let prompt = template
            .to_prompt()
            .with_context_value("outline", outline)
            .with_context_value("code", code);

        insta::assert_snapshot!(prompt.render().await.unwrap());
    }

    #[tokio::test]
    async fn test_compress_code_outline() {
        let mut client = MockSimplePrompt::new();

        client
            .expect_prompt()
            .returning(|_| Ok("RelevantOutline".to_string()));

        let transformer = CompressCodeOutline::builder()
            .client(client)
            .build()
            .unwrap();
        let mut node = Node::new("Some text");
        node.offset = 0;
        node.original_size = 100;

        node.metadata
            .insert("Outline".to_string(), "Some outline".to_string());

        let result = transformer.transform_node(node).await.unwrap();

        assert_eq!(result.chunk, "Some text");
        assert_eq!(result.metadata.get("Outline").unwrap(), "RelevantOutline");
    }
}
