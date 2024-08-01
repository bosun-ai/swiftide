//! Generate questions and answers based on code chunks and add them as metadata
use derive_builder::Builder;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::{indexing::Node, prompt::PromptTemplate, SimplePrompt, Transformer};

/// `CompressCodeChunk` rewrites the "Context (Code)" metadata field of a chunk to
/// condense it and make it more relevant to the chunk in question. It is useful as a
/// step after chunking a file that has had context generated for it with `FileToContextTreeSitter`.
#[derive(Debug, Clone, Builder)]
#[builder(setter(into, strip_option))]
pub struct CompressCodeContext {
    #[builder(setter(custom))]
    client: Arc<dyn SimplePrompt>,
    #[builder(default = "default_prompt()")]
    prompt_template: PromptTemplate,
    #[builder(default)]
    concurrency: Option<usize>,
}

fn extract_markdown_codeblock(text: String) -> String {
    let re = regex::Regex::new(r"(?sm)```\w*\n(.*?)```").unwrap();
    let captures = re.captures(text.as_str());
    captures
        .map(|c| c.get(1).unwrap().as_str().to_string())
        .unwrap_or(text)
}

impl CompressCodeContext {
    pub fn builder() -> CompressCodeContextBuilder {
        CompressCodeContextBuilder::default()
    }

    pub fn from_client(client: impl SimplePrompt + 'static) -> CompressCodeContextBuilder {
        CompressCodeContextBuilder::default()
            .client(client)
            .to_owned()
    }
    /// Creates a new instance of `CompressCodeContext`.
    ///
    /// # Arguments
    ///
    /// * `client` - An implementation of the `SimplePrompt` trait used to generate questions and answers.
    ///
    /// # Returns
    ///
    /// A new instance of `CompressCodeContext` with a default prompt and a default number of questions.
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

/// Returns the default prompt template for generating questions and answers.
///
/// This template includes placeholders for the number of questions and the code chunk.
///
/// # Returns
///
/// A string representing the default prompt template.
fn default_prompt() -> PromptTemplate {
    include_str!("prompts/compress_code_context.prompt.md").into()
}

impl CompressCodeContextBuilder {
    pub fn client(&mut self, client: impl SimplePrompt + 'static) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }
}

#[async_trait]
impl Transformer for CompressCodeContext {
    /// Asynchronously transforms an `Node` by reducing the size of the context to make it more relevant to the chunk.
    ///
    /// This method uses the `SimplePrompt` client to compress the context of the `Node` and updates the `Node` with the compressed context.
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
    #[tracing::instrument(skip_all, name = "transformers.compress_code_context")]
    async fn transform_node(&self, mut node: Node) -> Result<Node> {
        let maybe_context = node.metadata.get("Context (code)");

        let Some(context) = maybe_context else {
            return Ok(node);
        };

        // If the chunk is not smaller than the original size, we don't need to do operations on the context
        if node.chunk.len() >= node.original_size {
            return Ok(node);
        }

        let prompt = self
            .prompt_template
            .to_prompt()
            .with_context_value("context", context.as_str())
            .with_context_value("code", node.chunk.clone());

        let response = extract_markdown_codeblock(self.client.prompt(prompt).await?);

        node.metadata.insert("Context (code)".to_string(), response);

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

        let context = "Relevant Context";
        let code = "Code using context";

        let prompt = template
            .to_prompt()
            .with_context_value("context", context)
            .with_context_value("code", code);

        insta::assert_snapshot!(prompt.render().await.unwrap());
    }

    #[tokio::test]
    async fn test_compress_code_context() {
        let mut client = MockSimplePrompt::new();

        client
            .expect_prompt()
            .returning(|_| Ok("RelevantContext".to_string()));

        let transformer = CompressCodeContext::builder()
            .client(client)
            .build()
            .unwrap();
        let mut node = Node::new("Some text");
        node.offset = 0;
        node.original_size = 100;

        node.metadata
            .insert("Context (code)".to_string(), "Some context".to_string());

        let result = transformer.transform_node(node).await.unwrap();

        assert_eq!(result.chunk, "Some text");
        assert_eq!(
            result.metadata.get("Context (code)").unwrap(),
            "RelevantContext"
        );
    }
}
