//! Chunk code using tree-sitter
use anyhow::Result;
use async_trait::async_trait;
use derive_builder::Builder;

use crate::{
    indexing::Node,
    integrations::treesitter::{CodeSummarizer, SupportedLanguages},
    Transformer,
};

pub const NAME: &str = "Context (code)";

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
    /// - `lang`: The programming language to be used to parse the code. It should implement `TryInto<SupportedLanguages>`.
    ///
    /// # Returns
    /// - `Result<Self>`: Returns an instance of `FileToContextTreeSitter` if successful, otherwise returns an error.
    ///
    /// # Errors
    /// - Returns an error if the language is not supported or if the `CodeSummarizer` fails to build.
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
    /// Adds a context to the metadata of a `Node` containing code.
    ///
    /// # Parameters
    /// - `node`: The `Node` containing the code of which the context is to be generated.
    ///
    /// # Returns
    /// - `Node`: The same `Node` instances, with the metadata updated to include the generated context.
    ///
    /// # Errors
    /// - If the code summarizing fails, an error is sent downstream.
    #[tracing::instrument(skip_all, name = "transformers.file_to_context_tree_sitter")]
    async fn transform_node(&self, mut node: Node) -> Result<Node> {
        let summary_result = self.summarizer.summarize(&node.chunk)?;
        node.metadata.insert(NAME.into(), summary_result);
        Ok(node)
    }

    fn concurrency(&self) -> Option<usize> {
        self.concurrency
    }
}
