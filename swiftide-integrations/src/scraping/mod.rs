//! Scraping loader using and html to markdown transformer
mod html_to_markdown_transformer;
mod loader;

pub use html_to_markdown_transformer::HtmlToMarkdownTransformer;
pub use loader::ScrapingLoader;
