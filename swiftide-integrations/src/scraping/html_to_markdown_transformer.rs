use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use htmd::HtmlToMarkdown;

use swiftide_core::{indexing::Node, Transformer};

/// Transforms HTML content into markdown.
///
/// Useful for converting scraping results into markdown.
#[swiftide_macros::indexing_transformer(derive(skip_default, skip_debug))]
pub struct HtmlToMarkdownTransformer {
    /// The `HtmlToMarkdown` instance used to convert HTML to markdown.
    ///
    /// Sets a sane default, but can be customized.
    htmd: Arc<HtmlToMarkdown>,
}

impl Default for HtmlToMarkdownTransformer {
    fn default() -> Self {
        Self {
            htmd: HtmlToMarkdown::builder()
                .skip_tags(vec!["script", "style"])
                .build()
                .into(),
            concurrency: None,
            client: None,
            indexing_defaults: None,
        }
    }
}

impl std::fmt::Debug for HtmlToMarkdownTransformer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HtmlToMarkdownTransformer").finish()
    }
}

#[async_trait]
impl Transformer for HtmlToMarkdownTransformer {
    /// Converts the HTML content in the `Node` to markdown.
    ///
    /// Will Err the node if the conversion fails.
    #[tracing::instrument(skip_all, name = "transformer.html_to_markdown")]
    async fn transform_node(&self, node: Node) -> Result<Node> {
        let chunk = self.htmd.convert(&node.chunk)?;

        Node::build_from_other(&node).chunk(chunk).build()
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
        let node = Node::new("<h1>Hello, World!</h1>");
        let transformer = HtmlToMarkdownTransformer::default();
        let transformed_node = transformer.transform_node(node).await.unwrap();
        assert_eq!(transformed_node.chunk, "# Hello, World!");
    }
}
