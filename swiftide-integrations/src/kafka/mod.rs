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
//! let kafka = Kafka::builder()
//!     .client_config(ClientConfig::new())
//!     .topic("Hello Kafka")
//!     .build().unwrap();
//! ```

use anyhow::{Context, Result};
use derive_builder::Builder;
use rdkafka::{
    admin::{AdminClient, AdminOptions, NewTopic, TopicReplication},
    client::DefaultClientContext,
    consumer::{Consumer, StreamConsumer},
    producer::FutureProducer,
};
use swiftide_core::indexing::Node;

pub use rdkafka::config::ClientConfig;

mod loader;
mod persist;

#[derive(Clone, Builder)]
#[builder(setter(into, strip_option))]
pub struct Kafka {
    client_config: ClientConfig,
    topic: String,
    #[builder(default)]
    /// Customize the key used for persisting nodes
    persist_key_fn: Option<fn(&Node) -> Result<String>>,
    #[builder(default)]
    /// Customize the value used for persisting nodes
    persist_value_fn: Option<fn(&Node) -> Result<String>>,
    #[builder(default = "1")]
    partition: i32,
    #[builder(default = "1")]
    factor: i32,
}

impl std::fmt::Debug for Kafka {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kafka").field("topic", &self.topic).finish()
    }
}

impl Kafka {
    pub fn from_client_config(config: impl Into<ClientConfig>, topic: impl Into<String>) -> Kafka {
        Kafka {
            client_config: config.into(),
            topic: topic.into(),
            persist_key_fn: None,
            persist_value_fn: None,
            partition: 1,
            factor: 1,
        }
    }

    pub fn builder() -> KafkaBuilder {
        KafkaBuilder::default()
    }

    fn producer(&self) -> Result<FutureProducer<DefaultClientContext>> {
        self.client_config
            .create()
            .context("Failed to create producer")
    }

    async fn create_topic_if_not_exists(&self) -> Result<()> {
        let consumer: StreamConsumer = self
            .client_config
            .create()
            .context("Failed to create consumer")?;
        let metadata = consumer.fetch_metadata(Some(&self.topic), None)?;
        if metadata.topics().is_empty() {
            let admin_client: AdminClient<DefaultClientContext> = self
                .client_config
                .create()
                .context("Failed to create admin client")?;
            admin_client
                .create_topics(
                    vec![&NewTopic::new(
                        &self.topic,
                        self.partition,
                        TopicReplication::Fixed(self.factor),
                    )],
                    &AdminOptions::new(),
                )
                .await?;
        }
        Ok(())
    }

    /// Generates a ky for a given node to be persisted in Kafka.
    fn persist_key_for_node(&self, node: &Node) -> Result<String> {
        if let Some(key_fn) = self.persist_key_fn {
            key_fn(node)
        } else {
            let hash = node.id();
            Ok(format!("{}:{}", node.path.to_string_lossy(), hash))
        }
    }

    /// Generates a value for a given node to be persisted in Kafka.
    /// By default, the node is serialized as JSON.
    /// If a custom function is provided, it is used to generate the value.
    /// Otherwise, the node is serialized as JSON.
    fn persist_value_for_node(&self, node: &Node) -> Result<String> {
        if let Some(value_fn) = self.persist_value_fn {
            value_fn(node)
        } else {
            Ok(serde_json::to_string(node)?)
        }
    }
}
