//! Kafka is a distributed streaming platform.
//!
//! This module provides a Kafka loader for Swiftide and allows you to ingest
//! messages from Kafka topics and use them for RAG.
//!
//! Can be configured with [`ClientConfig`].
//!
//! # Example
//!
//! ```no_run
//! # use swiftide_integrations::kafka::*;
//! let loader = Kafka::builder()
//!     .client_config(ClientConfig::new())
//!     .topic("Hello Kafka")
//!     .build().unwrap();
//! ```

use derive_builder::Builder;
pub use rdkafka::config::ClientConfig;

mod loader;

#[derive(Debug, Clone, Builder)]
#[builder(setter(into, strip_option))]
pub struct Kafka {
    client_config: ClientConfig,
    topic: String,
}

impl Kafka {
    pub fn from_client_config(config: impl Into<ClientConfig>, topic: impl Into<String>) -> Kafka {
        Kafka {
            client_config: config.into(),
            topic: topic.into(),
        }
    }

    pub fn builder() -> KafkaBuilder {
        KafkaBuilder::default()
    }
}
