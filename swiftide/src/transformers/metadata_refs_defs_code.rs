//! Adds references and definitions found in code as metadata to chunks
//!
//! Uses tree-sitter to do the extractions. It tries to only get unique definitions and references,
//! and only references that are not local.
//!
//! See the [`integrations::treesitter::CodeParser`] tests for some examples.
use derive_builder::Builder;

use crate::{
    indexing::Node,
    integrations::treesitter::{CodeParser, SupportedLanguages},
    Transformer,
};
use anyhow::{Context as _, Result};
use async_trait::async_trait;

pub const NAME_REFERENCES: &str = "References (code)";
pub const NAME_DEFINITIONS: &str = "Definitions (code)";

/// `MetadataRefsDefsCode` is responsible for extracting references and definitions.
#[derive(Debug, Builder)]
#[builder(
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "anyhow::Error")
)]
pub struct MetadataRefsDefsCode {
    code_parser: CodeParser,

    #[builder(default)]
    concurrency: Option<usize>,
}

impl MetadataRefsDefsCode {
    /// Tries to build a new `MetadataRefsDefsCode` transformer
    ///
    /// # Errors
    ///
    /// Language is not supported by tree-sitter
    pub fn try_from_language(language: impl TryInto<SupportedLanguages>) -> Result<Self> {
        let language: SupportedLanguages = language
            .try_into()
            .ok()
            .context("Treesitter language not supported")?;

        MetadataRefsDefsCode::builder()
            .code_parser(CodeParser::from_language(language))
            .build()
    }

    pub fn builder() -> MetadataRefsDefsCodeBuilder {
        MetadataRefsDefsCodeBuilder::default()
    }

    #[must_use]
    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = Some(concurrency);
        self
    }
}

#[async_trait]
impl Transformer for MetadataRefsDefsCode {
    /// Extracts references and definitions from code and
    /// adds them as metadata to the node if present
    async fn transform_node(&self, mut node: Node) -> Result<Node> {
        let refs_defs = self
            .code_parser
            .parse(&node.chunk)?
            .references_and_definitions()?;

        if !refs_defs.references.is_empty() {
            node.metadata
                .insert(NAME_REFERENCES.to_string(), refs_defs.references.join(","));
        }

        if !refs_defs.definitions.is_empty() {
            node.metadata.insert(
                NAME_DEFINITIONS.to_string(),
                refs_defs.definitions.join(","),
            );
        }
        Ok(node)
    }
}
