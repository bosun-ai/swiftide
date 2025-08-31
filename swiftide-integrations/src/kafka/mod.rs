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
use swiftide_core::indexing::TextNode;

pub use rdkafka::config::ClientConfig;

mod loader;
mod persist;

#[derive(Debug, Clone, Builder)]
#[builder(setter(into, strip_option))]
pub struct Kafka {
    client_config: ClientConfig,
    topic: String,
    #[builder(default)]
    /// Customize the key used for persisting nodes
    persist_key_fn: Option<fn(&TextNode) -> Result<String>>,
    #[builder(default)]
    /// Customize the value used for persisting nodes
    persist_payload_fn: Option<fn(&TextNode) -> Result<String>>,
    #[builder(default = "1")]
    partition: i32,
    #[builder(default = "1")]
    factor: i32,
    #[builder(default)]
    create_topic_if_not_exists: bool,
    #[builder(default = "32")]
    batch_size: usize,
}

impl Kafka {
    pub fn from_client_config(config: impl Into<ClientConfig>, topic: impl Into<String>) -> Kafka {
        Kafka {
            client_config: config.into(),
            topic: topic.into(),
            persist_key_fn: None,
            persist_payload_fn: None,
            partition: 1,
            factor: 1,
            create_topic_if_not_exists: false,
            batch_size: 32,
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

    fn topic_exists(&self) -> Result<bool> {
        let consumer: StreamConsumer = self
            .client_config
            .create()
            .context("Failed to create consumer")?;
        let metadata = consumer.fetch_metadata(Some(&self.topic), None)?;
        Ok(!metadata.topics().is_empty())
    }

    async fn create_topic(&self) -> Result<()> {
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
        Ok(())
    }

    /// Generates a ky for a given node to be persisted in Kafka.
    fn persist_key_for_node(&self, node: &TextNode) -> Result<String> {
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
    fn persist_value_for_node(&self, node: &TextNode) -> Result<String> {
        if let Some(value_fn) = self.persist_payload_fn {
            value_fn(node)
        } else {
            Ok(serde_json::to_string(node)?)
        }
    }

    fn node_to_key_payload(&self, node: &TextNode) -> Result<(String, String)> {
        let key = self.persist_key_for_node(node).map_err(|e| {
            anyhow::anyhow!("persist_key_for_node failed: {:?} (node: {:?})", e, node)
        })?;
        let payload = self.persist_value_for_node(node).map_err(|e| {
            anyhow::anyhow!("persist_value_for_node failed: {:?} (node: {:?})", e, node)
        })?;

        Ok((key, payload))
    }
}
