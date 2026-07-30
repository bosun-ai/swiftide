use derive_builder::Builder;
use spider::configuration::{SpiderCloudConfig, SpiderCloudMode};
use spider::website::Website;

use swiftide_core::{
    Loader,
    indexing::{IndexingStream, TextNode},
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

/// Spider Cloud config resolved from the environment once, or `None` when
/// `SPIDER_CLOUD_API_KEY` is unset.
static SPIDER_CLOUD: std::sync::OnceLock<Option<SpiderCloudConfig>> = std::sync::OnceLock::new();

fn spider_cloud() -> Option<&'static SpiderCloudConfig> {
    SPIDER_CLOUD.get_or_init(spider_cloud_from_env).as_ref()
}

/// Spider Cloud fetches from its own infrastructure, so it cannot reach hosts
/// that are only routable from here. Those are left on the direct path.
fn is_locally_routable(url: &str) -> bool {
    use spider::url::Host;

    let Ok(parsed) = spider::url::Url::parse(url) else {
        return false;
    };

    match parsed.host() {
        Some(Host::Domain(host)) => {
            host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local")
        }
        Some(Host::Ipv4(ip)) => {
            ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified()
        }
        Some(Host::Ipv6(ip)) => ip.is_loopback() || ip.is_unspecified(),
        None => false,
    }
}

fn spider_cloud_from_env() -> Option<SpiderCloudConfig> {
    let api_key = std::env::var("SPIDER_CLOUD_API_KEY").ok()?;
    let api_key = api_key.trim();

    if api_key.is_empty() {
        return None;
    }

    // Anything unrecognized falls through to `Smart`, which proxies and
    // escalates to the unblocker only when it sees bot protection.
    let mode = match std::env::var("SPIDER_CLOUD_MODE")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "proxy" => SpiderCloudMode::Proxy,
        "api" => SpiderCloudMode::Api,
        "unblocker" => SpiderCloudMode::Unblocker,
        "fallback" => SpiderCloudMode::Fallback,
        _ => SpiderCloudMode::Smart,
    };

    let mut config = SpiderCloudConfig::new(api_key).with_mode(mode);

    if let Ok(api_url) = std::env::var("SPIDER_CLOUD_API_URL") {
        if !api_url.trim().is_empty() {
            config = config.with_api_url(api_url.trim());
        }
    }

    Some(config)
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
    type Output = String;

    fn into_stream(mut self) -> IndexingStream<String> {
        let (tx, rx) = tokio::sync::mpsc::channel(1000);

        if let Some(config) = spider_cloud() {
            if is_locally_routable(self.spider_website.get_url().inner()) {
                tracing::debug!("[Spider] Local host, skipping Spider Cloud");
            } else {
                tracing::info!(mode = ?config.mode, "[Spider] Using Spider Cloud");
                self.spider_website.with_spider_cloud_config(config.clone());
            }
        }

        let mut spider_rx = self.spider_website.subscribe(0);
        tracing::info!("Subscribed to spider");

        let _recv_thread = tokio::spawn(async move {
            while let Ok(res) = spider_rx.recv().await {
                let html = res.get_html();
                let original_size = html.len();

                let node = TextNode::builder()
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
            //
            // Boxed because spider's crawl future is large enough to trip
            // `clippy::large_futures` when held across an await in a task.
            Box::pin(spider_website.crawl()).await;
            tracing::info!("[Spider] Scrape loop finished");
        });

        // NOTE: Handles should stay alive because of rx, but feels a bit fishy
        rx.into()
    }

    fn into_stream_boxed(self: Box<Self>) -> IndexingStream<String> {
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

    #[test]
    fn test_is_locally_routable() {
        for url in [
            "http://localhost:8080",
            "http://127.0.0.1:3000",
            "http://[::1]:3000",
            "http://192.168.1.10",
            "http://10.0.0.1",
            "http://nas.local",
        ] {
            assert!(is_locally_routable(url), "{url} should be local");
        }

        for url in [
            "https://example.com",
            "https://books.toscrape.com",
            "http://1.1.1.1",
        ] {
            assert!(!is_locally_routable(url), "{url} should not be local");
        }
    }

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
        let nodes = stream.collect::<Vec<Result<TextNode>>>().await;

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
        let mut nodes = stream.collect::<Vec<Result<TextNode>>>().await;

        assert_eq!(nodes.len(), 2);

        let first_node = nodes.pop().unwrap().unwrap();

        assert_eq!(first_node.chunk, body2);

        let second_node = nodes.pop().unwrap().unwrap();

        assert_eq!(second_node.chunk, body);
    }
}
