//! Add the outline of the code in the given file to the metadata of a node, using tree-sitter.
use anyhow::Result;
use async_trait::async_trait;

use swiftide_core::indexing::Node;
use swiftide_core::Transformer;

use crate::treesitter::{CodeOutliner, SupportedLanguages};

/// `OutlineCodeTreeSitter` adds a "Outline" field to the metadata of a node that contains
/// a summary of the code in the node. It uses the tree-sitter parser to parse the code and
/// remove any information that is less relevant for tasks that consider the file as a whole.
#[swiftide_macros::indexing_transformer(metadata_field_name = "Outline", derive(skip_default))]
pub struct OutlineCodeTreeSitter {
    outliner: CodeOutliner,
    minimum_file_size: Option<usize>,
}

impl OutlineCodeTreeSitter {
    /// Tries to create a `OutlineCodeTreeSitter` instance for a given programming language.
    ///
    /// # Parameters
    /// - `lang`: The programming language to be used to parse the code. It should implement
    ///   `TryInto<SupportedLanguages>`.
    ///
    /// # Returns
    /// - `Result<Self>`: Returns an instance of `OutlineCodeTreeSitter` if successful, otherwise
    ///   returns an error.
    ///
    /// # Errors
    /// - Returns an error if the language is not supported or if the `CodeOutliner` fails to build.
    pub fn try_for_language(
        lang: impl TryInto<SupportedLanguages>,
        minimum_file_size: Option<usize>,
    ) -> Result<Self> {
        Ok(Self {
            outliner: CodeOutliner::builder().try_language(lang)?.build()?,
            minimum_file_size,
            client: None,
            concurrency: None,
            indexing_defaults: None,
        })
    }
}

#[async_trait]
impl Transformer for OutlineCodeTreeSitter {
    /// Adds context to the metadata of a `Node` containing code in the "Outline" field.
    ///
    /// It uses the `CodeOutliner` to generate the context.
    ///
    /// # Parameters
    /// - `node`: The `Node` containing the code of which the context is to be generated.
    ///
    /// # Returns
    /// - `Node`: The same `Node` instances, with the metadata updated to include the generated
    ///   context.
    ///
    /// # Errors
    /// - If the code outlining fails, an error is sent downstream.
    #[tracing::instrument(skip_all, name = "transformers.outline_code_tree_sitter")]
    async fn transform_node(&self, mut node: Node) -> Result<Node> {
        if let Some(minimum_file_size) = self.minimum_file_size {
            if node.chunk.len() < minimum_file_size {
                return Ok(node);
            }
        }

        let outline_result = self.outliner.outline(&node.chunk)?;
        node.metadata.insert(NAME, outline_result);
        Ok(node)
    }
}
