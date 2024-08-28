use anyhow::Context as _;
use anyhow::Result;
use deadpool::managed::Manager;
use derive_builder::Builder;
use lancedb::connection::ConnectBuilder;

#[derive(Builder, Debug, Clone)]
#[builder(setter(into), build_fn(error = "anyhow::Error"))]
pub struct LanceDBPoolManager {
    uri: String,
    api_key: Option<String>,
    region: Option<String>,
    storage_options: Vec<(String, String)>,
}

pub type LanceDBConnectionPool = deadpool::managed::Pool<LanceDBPoolManager>;

impl LanceDBPoolManager {
    pub fn builder() -> LanceDBPoolManagerBuilder {
        LanceDBPoolManagerBuilder::default()
    }
}

// impl LanceDBConnectionPoolBuilder {
//     fn default_pool_manager(&self) -> Result<LanceDBPoolManager> {
//         LanceDBPoolManagerBuilder::default()
//             .uri(self.uri.clone().context("Expected URI")?)
//             .api_key(self.api_key.clone().flatten())
//             .region(self.region.clone().flatten())
//             .storage_options(self.storage_options.clone().unwrap_or_default())
//             .build()
//     }
// }

impl Manager for LanceDBPoolManager {
    type Type = lancedb::Connection;
    type Error = anyhow::Error;

    async fn create(&self) -> Result<Self::Type, Self::Error> {
        let mut builder = ConnectBuilder::new(&self.uri);

        if let Some(api_key) = &self.api_key {
            builder = builder.api_key(api_key);
        }

        if let Some(region) = &self.region {
            builder = builder.region(region);
        }

        for (key, value) in &self.storage_options {
            builder = builder.storage_option(key, value);
        }

        builder
            .execute()
            .await
            .context("Failed to create LanceDB connection")
    }

    async fn recycle(
        &self,
        _obj: &mut Self::Type,
        _metrics: &deadpool::managed::Metrics,
    ) -> deadpool::managed::RecycleResult<Self::Error> {
        // NOTE: Should work fine with drop
        Ok(())
    }
}
