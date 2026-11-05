//! A custom Swiftide [`Loader`] that turns documentation sources into one node per page.

use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{Context as _, Result};
use futures_util::{StreamExt as _, TryStreamExt as _, stream};
use swiftide::indexing::{IndexingStream, Loader, TextNode, loaders::FileLoader};

use crate::docs::{PAGE_TITLE, Page};

/// Where documentation comes from.
#[derive(Debug, Clone)]
pub enum DocsSource {
    /// A directory, walked for `*.md` files (respects `.gitignore`).
    MarkdownDir(PathBuf),
    /// A single markdown file.
    MarkdownFile(PathBuf),
    /// An `llms-full.txt` file on disk; split into pages.
    LlmsFullFile(PathBuf),
    /// An `llms-full.txt` file on the web; split into pages.
    LlmsFullUrl(String),
}

impl FromStr for DocsSource {
    type Err = anyhow::Error;

    fn from_str(source: &str) -> Result<Self> {
        if source.starts_with("http://") || source.starts_with("https://") {
            return Ok(Self::LlmsFullUrl(source.to_string()));
        }

        let path = PathBuf::from(source);
        if path.is_dir() {
            Ok(Self::MarkdownDir(path))
        } else if !path.is_file() {
            anyhow::bail!("{source} is not a directory, file or URL")
        } else if path.extension().is_some_and(|ext| ext == "md") {
            Ok(Self::MarkdownFile(path))
        } else {
            Ok(Self::LlmsFullFile(path))
        }
    }
}

impl DocsSource {
    async fn pages(self) -> Result<Vec<Page>> {
        match self {
            Self::MarkdownDir(dir) => markdown_pages_in(dir).await,
            Self::MarkdownFile(path) => {
                let markdown = read(&path).await?;
                Ok(vec![Page::from_markdown_file(display(&path), markdown)])
            }
            Self::LlmsFullFile(path) => {
                let text = read(&path).await?;
                Ok(Page::split_llms_full(&display(&path), &text))
            }
            Self::LlmsFullUrl(url) => {
                let text = reqwest::get(&url)
                    .await
                    .and_then(reqwest::Response::error_for_status)
                    .with_context(|| format!("Failed to download {url}"))?
                    .text()
                    .await?;
                Ok(Page::split_llms_full(&url, &text))
            }
        }
    }
}

// ANCHOR: loader
/// Yields one [`TextNode`] per documentation page, with the page title as metadata.
#[derive(Debug, Clone)]
pub struct DocsLoader {
    sources: Vec<DocsSource>,
}

impl DocsLoader {
    pub fn new(sources: Vec<DocsSource>) -> Self {
        Self { sources }
    }
}

impl Loader for DocsLoader {
    type Output = String;

    fn into_stream(self) -> IndexingStream<String> {
        stream::iter(self.sources)
            .then(DocsSource::pages)
            .map_ok(|pages| stream::iter(pages).map(|page| Ok(page_node(page))))
            .try_flatten()
            .boxed()
            .into()
    }

    // Only needed when the loader is used as a `Box<dyn Loader>`.
    fn into_stream_boxed(self: Box<Self>) -> IndexingStream<String> {
        self.into_stream()
    }
}

fn page_node(page: Page) -> TextNode {
    let mut node = TextNode::new(page.markdown);
    node.path = page.origin.into();
    node.metadata.insert(PAGE_TITLE, page.title.as_str());
    node
}
// ANCHOR_END: loader

/// Reuses Swiftide's `FileLoader` to walk a directory; each file becomes a page.
async fn markdown_pages_in(dir: PathBuf) -> Result<Vec<Page>> {
    tokio::task::spawn_blocking(move || {
        FileLoader::new(&dir)
            .with_extensions(&["md"])
            .iter()
            .map(|file| file.map(|node| Page::from_markdown_file(display(&node.path), node.chunk)))
            .collect()
    })
    .await?
}

async fn read(path: &Path) -> Result<String> {
    tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("Failed to read {}", path.display()))
}

fn display(path: &Path) -> String {
    path.display().to_string()
}
