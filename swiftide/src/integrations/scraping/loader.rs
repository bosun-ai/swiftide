#![cfg(not(tarpaulin_include))]
use derive_builder::Builder;
use spider::website::Website;
use tokio::{runtime::Handle, sync::RwLock};

use crate::{
    ingestion::{IndexingStream, Node},
    Loader,
};

#[derive(Debug, Builder)]
#[builder(pattern = "owned")]
/// Scrapes a given website
///
/// Under the hood uses the `spider` crate to scrape the website.
/// For more configuration options see their documentation.
pub struct ScrapingLoader {
    spider_website: RwLock<Website>,
}

impl ScrapingLoader {
    pub fn builder() -> ScrapingLoaderBuilder {
        ScrapingLoaderBuilder::default()
    }

    // Constructs a scrapingloader from a `spider::Website` configuration
    #[allow(dead_code)]
    pub fn from_spider(spider_website: Website) -> Self {
        Self {
            spider_website: RwLock::new(spider_website),
        }
    }

    /// Constructs a scrapingloader from a given url
    pub fn from_url(url: impl AsRef<str>) -> Self {
        Self::from_spider(Website::new(url.as_ref()))
    }
}

impl Loader for ScrapingLoader {
    fn into_stream(self) -> IndexingStream {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut spider_rx = tokio::task::block_in_place(|| {
            Handle::current().block_on(async {
                self.spider_website
                    .write()
                    .await
                    .subscribe(0)
                    .expect("Failed to subscribe to spider")
            })
        });

        let _recv_thread = tokio::spawn(async move {
            while let Ok(res) = spider_rx.recv().await {
                let node = Node {
                    chunk: res.get_html(),
                    // TODO: Probably not the best way to represent this
                    // and will fail. Can we add more metadata too?
                    path: res.get_url().into(),
                    ..Default::default()
                };
                if tx.send(Ok(node)).is_err() {
                    break;
                }
            }
        });

        let _scrape_thread = tokio::spawn(async move {
            let mut spider_website = self.spider_website.write().await;
            spider_website.scrape().await;
        });

        // NOTE: Handles should stay alive because of rx, but feels a bit fishy

        IndexingStream::iter(rx)
    }
}
