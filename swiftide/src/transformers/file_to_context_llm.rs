//! Generate a summary or overview of a file that easily fits into LLM contexts by using an LLM to generate the summary.
use anyhow::{Context, Result};
use async_trait::async_trait;
use derive_builder::Builder;
use std::sync::Arc;

use crate::{indexing::Node, prompt::PromptTemplate, SimplePrompt, Transformer};

pub const NAME: &str = "Context (code)";

#[derive(Debug, Clone, Builder)]
#[builder(setter(into, strip_option))]
pub struct FileToContextLLM {
    #[builder(setter(custom))]
    client: Arc<dyn SimplePrompt>,
    #[builder(default = "default_initial_prompt()")]
    initial_prompt_template: PromptTemplate,
    #[builder(default = "default_subsequent_prompt()")]
    subsequent_prompt_template: PromptTemplate,
    #[builder(default = "2000")]
    max_context_size: usize,
    #[builder(default = "2000")]
    chunk_size: usize,
    #[builder(default)]
    concurrency: Option<usize>,
}

impl FileToContextLLM {
    pub fn builder() -> FileToContextLLMBuilder {
        FileToContextLLMBuilder::default()
    }

    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = Some(concurrency);
        self
    }

    pub fn from_client(client: impl SimplePrompt + 'static) -> FileToContextLLMBuilder {
        FileToContextLLMBuilder::default().client(client).to_owned()
    }

    pub fn new(client: impl SimplePrompt + 'static) -> Self {
        Self {
            client: Arc::new(client),
            initial_prompt_template: default_initial_prompt(),
            subsequent_prompt_template: default_subsequent_prompt(),
            max_context_size: 2000,
            chunk_size: 2000,
            concurrency: None,
        }
    }
}

impl FileToContextLLMBuilder {
    pub fn client(&mut self, client: impl SimplePrompt + 'static) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }
}

#[async_trait]
impl Transformer for FileToContextLLM {
    /// Uses an LLM to generate a summary of the file that fits into an LLM context.
    #[tracing::instrument(skip_all, name = "transformer.file_to_context_llm")]
    async fn transform_node(&self, mut node: Node) -> Result<Node> {
        let file_name = node
            .path
            .file_name()
            .context("No filename set")?
            .to_str()
            .context("Invalid filename")?;

        let whole_file = &node.chunk;
        let mut summary = String::new();
        let mut start = 0;
        let mut end = 0;
        let chunk_size = self.chunk_size;

        let mut current_chunk = &whole_file[start..end];
        let mut _previous_chunk: &str;
        let _max_context_size = self.max_context_size;

        while end < whole_file.len() {
            end = start + chunk_size;
            if end > whole_file.len() {
                end = whole_file.len();
            }
            _previous_chunk = current_chunk;
            current_chunk = &whole_file[start..end];

            if start == 0 {
                let prompt = self
                    .initial_prompt_template
                    .to_prompt()
                    .with_context_value("file_name", file_name)
                    .with_context_value("current_chunk", current_chunk);

                let response = self.client.prompt(prompt).await?;
                summary.push_str(&response);
            } else {
                let prompt = self
                    .subsequent_prompt_template
                    .to_prompt()
                    .with_context_value("summary_so_far", summary.clone())
                    .with_context_value("current_chunk", current_chunk);

                let response = self.client.prompt(prompt).await?;
                summary.push_str(&response);
            }
            start = end;
        }

        node.metadata.insert(NAME.into(), summary);

        Ok(node)
    }

    fn concurrency(&self) -> Option<usize> {
        self.concurrency
    }
}

fn default_initial_prompt() -> PromptTemplate {
    PromptTemplate::from_compiled_template_name(
        "src/transformers/prompts/file_to_context_initial.prompt.md",
    )
}

fn default_subsequent_prompt() -> PromptTemplate {
    PromptTemplate::from_compiled_template_name(
        "src/transformers/prompts/file_to_context_subsequent.prompt.md",
    )
}

#[cfg(test)]
mod test {
    use crate::MockSimplePrompt;

    use super::*;

    use std::path::PathBuf;

    #[tokio::test]
    async fn test_file_to_context_llm() {
        let mut client = MockSimplePrompt::new();

        client
            .expect_prompt()
            // .withf(|s| s.contains("example.py") && s.contains("1234567890"))
            .returning(|_| Ok("INITIAL_SUMMARY".to_string()));

        client
            .expect_prompt()
            // .withf(|s| s.contains("INITIAL_SUMMARY") && s.contains("ABCDEF"))
            .returning(|_| Ok("SUBSEQUENT_SUMMARY".to_string()));

        let transformer = FileToContextLLM::builder()
            .client(client)
            .chunk_size(10usize)
            .build()
            .unwrap();
        let mut node = Node::new("1234567890ABCDEFGHIJ");
        node.path = PathBuf::from("example.py");

        let result = transformer.transform_node(node).await.unwrap();

        assert_eq!(
            result.metadata.get(NAME).unwrap(),
            "INITIAL_SUMMARYSUBSEQUENT_SUMMARY"
        );
    }
}
