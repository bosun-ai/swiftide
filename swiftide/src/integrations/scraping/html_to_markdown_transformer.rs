use anyhow::Result;
use async_trait::async_trait;
use derive_builder::Builder;
use htmd::HtmlToMarkdown;

use crate::{ingestion::IngestionNode, Transformer};

#[derive(Builder)]
#[builder(pattern = "owned")]
/// Transforms HTML content into markdown.
///
/// Useful for converting scraping results into markdown.
pub struct HtmlToMarkdownTransformer {
    /// The `HtmlToMarkdown` instance used to convert HTML to markdown.
    ///
    /// Sets a sane default, but can be customized.
    htmd: HtmlToMarkdown,
    #[builder(default)]
    concurrency: Option<usize>,
}

impl Default for HtmlToMarkdownTransformer {
    fn default() -> Self {
        Self {
            htmd: HtmlToMarkdown::builder()
                .skip_tags(vec!["script", "style"])
                .build(),
            concurrency: None,
        }
    }
}

impl HtmlToMarkdownTransformer {
    #[allow(dead_code)]
    pub fn builder() -> HtmlToMarkdownTransformerBuilder {
        HtmlToMarkdownTransformerBuilder::default()
    }
}

impl std::fmt::Debug for HtmlToMarkdownTransformer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HtmlToMarkdownTransformer").finish()
    }
}

#[async_trait]
impl Transformer for HtmlToMarkdownTransformer {
    /// Converts the HTML content in the `IngestionNode` to markdown.
    ///
    /// Will Err the node if the conversion fails.
    #[tracing::instrument(skip_all, name = "transformer.html_to_markdown")]
    async fn transform_node(&self, node: IngestionNode) -> Result<IngestionNode> {
        let chunk = self.htmd.convert(&node.chunk);
        Ok(IngestionNode {
            chunk: chunk?,
            ..node
        })
    }

    fn concurrency(&self) -> Option<usize> {
        self.concurrency
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn test_html_to_markdown() {
        let node = IngestionNode::new("<h1>Hello, World!</h1>");
        let transformer = HtmlToMarkdownTransformer::default();
        let transformed = transformer.transform_node(node).await.unwrap();
        assert_eq!(transformed.chunk, "# Hello, World!");
    }
}
