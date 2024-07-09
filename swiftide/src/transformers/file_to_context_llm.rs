//! Generate a summary or overview of a file that easily fits into LLM contexts by using an LLM to generate the summary.
use anyhow::{Context, Result};
use async_trait::async_trait;
use derive_builder::Builder;
use indoc::indoc;
use std::sync::Arc;

use crate::{ingestion::IngestionNode, SimplePrompt, Transformer};

#[derive(Debug, Clone, Builder)]
#[builder(setter(into, strip_option))]
pub struct FileToContextLLM {
    #[builder(setter(custom))]
    client: Arc<dyn SimplePrompt>,
    #[builder(default = "default_initial_prompt()")]
    initial_prompt: String,
    #[builder(default = "default_subsequent_prompt()")]
    subsequent_prompt: String,
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
            initial_prompt: default_initial_prompt(),
            subsequent_prompt: default_subsequent_prompt(),
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
    async fn transform_node(&self, node: IngestionNode) -> Result<IngestionNode> {
        let file_name = node
            .path
            .file_name()
            .context("No filename set")?
            .to_str()
            .context("Invalid filename")?;

        let whole_file = node.chunk;
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
                    .initial_prompt
                    .replace("{file_name}", file_name)
                    .replace("{current_chunk}", current_chunk);

                let response = self.client.prompt(&prompt).await?;
                summary.push_str(&response);
            } else {
                let prompt = self
                    .subsequent_prompt
                    .replace("{summary_so_far}", &summary)
                    // .replace("{previous_chunk}", previous_chunk)
                    .replace("{current_chunk}", current_chunk);

                let response = self.client.prompt(&prompt).await?;
                summary.push_str(&response);
            }
            start = end;
        }

        Ok(IngestionNode {
            chunk: summary,
            ..node
        })
    }

    fn concurrency(&self) -> Option<usize> {
        self.concurrency
    }
}

fn default_prompt_task_description() -> String {
    indoc! {r#"
        # Task
        Your task is to produce a compact representation of the symbols defined in the code.

        You will be given chunks of a file, and your response should that same code chunk but stripped of function
        bodies, comments, and other non-essential parts. If comments contain important information, you should
        include summarized versions of them in your response. On the first line of the chunk, you will be given
        the name of the file.

        As a special rule the first line of your response should be the programming language of the code followed by
        the name of the file.

        Another special rule is that should the chunk end halfway through a function definition, you should end your
        response with the keyword "PARTIAL" to indicate that the function definition is incomplete.

        "#
    }.to_string()
}

fn default_initial_prompt() -> String {
    default_prompt_task_description()
        + indoc! {r#"

            For example, given the following code:
            ```
            example.py
            import hashlib

            def foo():
                # This is a comment
                return 1

            def bar():
                if True:
            ```

            Your response should be:
            ```
            Python example.py
            import hashlib
            def foo():
            def bar():
            PARTIAL
            ```

            Another example, now in Java:
            ```
            example.java
            import java.util.*;

            public class Main {
                public static void main(String[] args) {
                    // This is a comment
                    System.out.println("Hello, World!");
                }
            }
            ```

            Your response should be:
            ```
            Java example.java
            import java.util.*;
            public class Main {
                public static void main(String[] args)
            }
            ```

            This is the first chunk of code, please give your response following the rules above without further
            commentary or explanation:

            ```
            {file_name}
            {current_chunk}
            ```

        "#}
}

fn default_subsequent_prompt() -> String {
    default_prompt_task_description()
        + indoc! {r#"

            This task has already been performed on the previous chunks of code. The summary so far is:

            ```
            {summary_so_far}
            ```

            This is the next chunk of code, please give your response following the rules above without further
            commentary or explanation:

            ```
            {current_chunk}
            ```
        "#}
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
            .withf(|s| s.contains("example.py") && s.contains("1234567890"))
            .returning(|_| Ok("INITIAL_SUMMARY".to_string()));

        client
            .expect_prompt()
            .withf(|s| s.contains("INITIAL_SUMMARY") && s.contains("ABCDEF"))
            .returning(|_| Ok("SUBSEQUENT_SUMMARY".to_string()));

        let transformer = FileToContextLLM::builder()
            .client(client)
            .chunk_size(10usize)
            .build()
            .unwrap();
        let mut node = IngestionNode::new("1234567890ABCDEFGHIJ");
        node.path = PathBuf::from("example.py");

        let result = transformer.transform_node(node).await.unwrap();

        assert_eq!(result.chunk, "INITIAL_SUMMARYSUBSEQUENT_SUMMARY");
    }
}
