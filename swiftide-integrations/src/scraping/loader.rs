use derive_builder::Builder;
use spider::website::Website;

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
        let (tx, rx) = tokio::sync::mpsc::channel(1000);
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

                if let Err(error) = tx.send(node).await {
                    tracing::error!(?error, "[Spider] Failed to send node to stream");
                    break;
                }
            }
        });

        let mut spider_website = self.spider_website;

        let _scrape_thread = tokio::spawn(async move {
            tracing::info!("[Spider] Starting scrape loop");
            // TODO: It would be much nicer if this used `scrape` instead, as it is supposedly
            // more concurrent
            spider_website.crawl().await;
            tracing::info!("[Spider] Scrape loop finished");
        });

        // NOTE: Handles should stay alive because of rx, but feels a bit fishy
        rx.into()
    }

    fn into_stream_boxed(self: Box<Self>) -> IndexingStream {
        self.into_stream()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use futures_util::StreamExt;
    use swiftide_core::indexing::Loader;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

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
        let stream = loader.into_stream();

        // Process the stream to check if we get the expected result
        let nodes = stream.collect::<Vec<Result<Node>>>().await;

        assert_eq!(nodes.len(), 1);

        let first_node = nodes.first().unwrap().as_ref().unwrap();

        assert_eq!(first_node.chunk, body);
    }

    #[test_log::test(tokio::test(flavor = "multi_thread"))]
    async fn test_scraping_loader_multiple_pages() {
        // Set up the wiremock server to simulate the remote web server
        let mock_server = MockServer::start().await;

        // Mocked response for the page we will scrape
        let body = "<html><body><h1>Test Page</h1><a href=\"/other\">link</a></body></html>";
        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&mock_server)
            .await;

        let body2 = "<html><body><h1>Test Page 2</h1></body></html>";
        Mock::given(method("GET"))
            .and(path("/other"))
            .respond_with(move |_req: &Request| {
                std::thread::sleep(std::time::Duration::from_secs(1));
                ResponseTemplate::new(200).set_body_string(body2)
            })
            .mount(&mock_server)
            .await;

        // Create an instance of ScrapingLoader using the mock server's URL
        let loader = ScrapingLoader::from_url(mock_server.uri());

        // Execute the into_stream method
        let stream = loader.into_stream();

        // Process the stream to check if we get the expected result
        let mut nodes = stream.collect::<Vec<Result<Node>>>().await;

        assert_eq!(nodes.len(), 2);

        let first_node = nodes.pop().unwrap().unwrap();

        assert_eq!(first_node.chunk, body2);

        let second_node = nodes.pop().unwrap().unwrap();

        assert_eq!(second_node.chunk, body);
    }
}
