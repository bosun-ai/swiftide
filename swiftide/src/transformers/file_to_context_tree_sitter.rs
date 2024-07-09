//! Chunk code using tree-sitter
use anyhow::Result;
use async_trait::async_trait;
use derive_builder::Builder;

use crate::{
    ingestion::IngestionNode,
    integrations::treesitter::{CodeSummarizer, SupportedLanguages},
    Transformer,
};

#[derive(Debug, Clone, Builder)]
#[builder(pattern = "owned", setter(into, strip_option))]
pub struct FileToContextTreeSitter {
    summarizer: CodeSummarizer,
    #[builder(default)]
    concurrency: Option<usize>,
}

impl FileToContextTreeSitter {
    pub fn builder() -> FileToContextTreeSitterBuilder {
        FileToContextTreeSitterBuilder::default()
    }

    /// Tries to create a `FileToContextTreeSitter` instance for a given programming language.
    ///
    /// # Parameters
    /// - `lang`: The programming language to be used for chunking. It should implement `TryInto<SupportedLanguages>`.
    ///
    /// # Returns
    /// - `Result<Self>`: Returns an instance of `FileToContextTreeSitter` if successful, otherwise returns an error.
    ///
    /// # Errors
    /// - Returns an error if the language is not supported or if the `CodeSplitter` fails to build.
    pub fn try_for_language(lang: impl TryInto<SupportedLanguages>) -> Result<Self> {
        Ok(Self {
            summarizer: CodeSummarizer::builder().try_language(lang)?.build()?,
            concurrency: None,
        })
    }

    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = Some(concurrency);
        self
    }
}

#[async_trait]
impl Transformer for FileToContextTreeSitter {
    /// Transforms an `IngestionNode` by splitting its code chunk into smaller pieces.
    ///
    /// # Parameters
    /// - `node`: The `IngestionNode` containing the code chunk to be split.
    ///
    /// # Returns
    /// - `IngestionStream`: A stream of `IngestionNode` instances, each containing a smaller chunk of code.
    ///
    /// # Errors
    /// - If the code splitting fails, an error is sent downstream.
    #[tracing::instrument(skip_all, name = "transformers.chunk_code")]
    async fn transform_node(&self, node: IngestionNode) -> Result<IngestionNode> {
        let summary_result = self.summarizer.summarize(&node.chunk);

        if let Ok(summary) = summary_result {
            Ok(IngestionNode {
                chunk: summary,
                ..node
            })
        } else {
            // Send the error downstream
            Err(summary_result.unwrap_err())
        }
    }

    fn concurrency(&self) -> Option<usize> {
        self.concurrency
    }
}
