use std::sync::Arc;

use derive_builder::Builder;
use spider::website::Website;
use tokio::{runtime::Handle, sync::RwLock};

use swiftide_core::{
    indexing::{IndexingStream, Node},
    Loader,
};

#[derive(Debug, Builder, Clone)]
#[builder(pattern = "owned")]
/// Scrapes a given website
///
/// Under the hood uses the `spider` crate to scrape the website.
/// For more configuration options see their documentation.
pub struct ScrapingLoader {
    spider_website: Website,
}

impl ScrapingLoader {
    pub fn builder() -> ScrapingLoaderBuilder {
        ScrapingLoaderBuilder::default()
    }

    // Constructs a scrapingloader from a `spider::Website` configuration
    #[allow(dead_code)]
    pub fn from_spider(spider_website: Website) -> Self {
        Self { spider_website }
    }

    /// Constructs a scrapingloader from a given url
    pub fn from_url(url: impl AsRef<str>) -> Self {
        Self::from_spider(Website::new(url.as_ref()))
    }
}

impl Loader for ScrapingLoader {
    fn into_stream(mut self) -> IndexingStream {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut spider_rx = self
            .spider_website
            .subscribe(0)
            .expect("Failed to subscribe to spider");
        tracing::info!("Subscribed to spider");

        let _recv_thread = tokio::spawn(async move {
            while let Ok(res) = spider_rx.recv().await {
                let html = res.get_html();
                let original_size = html.len();

                let node = Node::builder()
                    .chunk(html)
                    .original_size(original_size)
                    .path(res.get_url())
                    .build();

                tracing::debug!(?node, "[Spider] Received node from spider");

                if let Err(error) = tx.send(node) {
                    tracing::error!(?error, "[Spider] Failed to send node to stream");
                    break;
                }
            }
        });

        let mut spider_website = self.spider_website;

        let _scrape_thread = tokio::spawn(async move {
            tracing::info!("[Spider] Starting scrape loop");
            spider_website.scrape().await;
            spider_website.unsubscribe();
            tracing::info!("[Spider] Scrape loop finished");
        });

        // NOTE: Handles should stay alive because of rx, but feels a bit fishy

        IndexingStream::iter(rx)
    }

    fn into_stream_boxed(self: Box<Self>) -> IndexingStream {
        self.into_stream()
    }
}

#[cfg(test)]
mod tests {
    use crate::scraping::loader::ScrapingLoader;
    use futures_util::StreamExt as _;
    use swiftide_core::indexing::Loader;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test_log::test(tokio::test(flavor = "multi_thread"))]
    async fn test_scraping_loader_with_wiremock() {
        // Set up the wiremock server to simulate the remote web server
        let mock_server = MockServer::start().await;

        // Mocked response for the page we will scrape
        let body = "<html><body><h1>Test Page</h1></body></html>";
        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&mock_server)
            .await;

        // Create an instance of ScrapingLoader using the mock server's URL
        let loader = ScrapingLoader::from_url(mock_server.uri());

        // Execute the into_stream method
        let mut stream = loader.into_stream();

        // Process the stream to check if we get the expected result
        while let Some(node) = stream.next().await {
            tracing::info!(?node, "Received node from stream");
            // Assert the scraped content against expected content
            assert_eq!(node.unwrap().chunk, body);
        }

        tracing::info!("Stream finished");

        drop(stream);
    }
}
