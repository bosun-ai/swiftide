//! This module provides the integration with Redis for caching nodes in the Swiftide system.
//!
//! The primary component of this module is the `Redis`, which is re-exported for use
//! in other parts of the system. The `Redis` struct is responsible for managing and
//! caching nodes during the indexing process, leveraging Redis for efficient storage and retrieval.
//!
//! # Overview
//!
//! Redis implements the following `Swiftide` traits:
//! - `Node<T>Cache`
//! - `Persist`
//! - `MessageHistory`
//!
//! Additionally it provides various helper and utility functions for managing the Redis connection
//! and key management. The connection is managed using a connection manager. When
//! cloned, the connection manager is shared across all instances.

use std::sync::Arc;

use anyhow::{Context as _, Result};
use derive_builder::Builder;
use serde::Serialize;
use tokio::sync::RwLock;

use swiftide_core::indexing::{Chunk, Node};

mod message_history;
mod node_cache;
mod persist;

/// `Redis` provides a caching mechanism for nodes using Redis.
/// It helps in optimizing the indexing process by skipping nodes that have already been processed.
///
/// # Fields
///
/// * `client` - The Redis client used to interact with the Redis server.
/// * `connection_manager` - Manages the Redis connections asynchronously.
/// * `key_prefix` - A prefix used for keys stored in Redis to avoid collisions.
#[allow(clippy::type_complexity)]
#[derive(Builder, Clone)]
#[builder(pattern = "owned", setter(strip_option))]
pub struct Redis<T: Chunk = String> {
    #[builder(setter(into))]
    client: Arc<redis::Client>,
    #[builder(default, setter(skip))]
    connection_manager: Arc<RwLock<Option<redis::aio::ConnectionManager>>>,
    #[builder(default, setter(into))]
    cache_key_prefix: Arc<String>,
    #[builder(default = "10")]
    /// The batch size used for persisting nodes. Defaults to a safe 10.
    batch_size: usize,
    #[builder(default)]
    /// Customize the key used for persisting nodes
    persist_key_fn: Option<fn(&Node<T>) -> Result<String>>,
    #[builder(default)]
    /// Customize the value used for persisting nodes
    persist_value_fn: Option<fn(&Node<T>) -> Result<String>>,
    #[builder(default = "message_history".to_string().into(), setter(into))]
    message_history_key: Arc<String>,
}

impl Redis<String> {
    /// Creates a new `Redis` instance from a given Redis URL and key prefix.
    ///
    /// # Parameters
    ///
    /// * `url` - The URL of the Redis server.
    /// * `prefix` - The prefix to be used for keys stored in Redis.
    ///
    /// # Returns
    ///
    /// A `Result` containing the `Redis` instance or an error if the client could not be created.
    ///
    /// # Errors
    ///
    /// Returns an error if the Redis client cannot be opened.
    pub fn try_from_url(url: impl AsRef<str>, prefix: impl AsRef<str>) -> Result<Redis<String>> {
        let client = redis::Client::open(url.as_ref()).context("Failed to open redis client")?;
        Ok(Redis::<String> {
            client: client.into(),
            connection_manager: Arc::new(RwLock::new(None)),
            cache_key_prefix: prefix.as_ref().to_string().into(),
            batch_size: 10,
            persist_key_fn: None,
            persist_value_fn: None,
            message_history_key: format!("{}:message_history", prefix.as_ref()).into(),
        })
    }
}

impl<T: Chunk> Redis<T> {
    /// # Errors
    ///
    /// Returns an error if the Redis client cannot be opened
    pub fn try_build_from_url(url: impl AsRef<str>) -> Result<RedisBuilder<T>> {
        Ok(RedisBuilder::default()
            .client(redis::Client::open(url.as_ref()).context("Failed to open redis client")?))
    }

    /// Builds a new `Redis` instance from the builder.
    pub fn builder() -> RedisBuilder<T> {
        RedisBuilder::default()
    }

    /// Set the key to be used for the message history
    pub fn with_message_history_key(&mut self, prefix: impl Into<String>) -> &mut Self {
        self.message_history_key = Arc::new(prefix.into());
        self
    }

    /// Lazily connects to the Redis server and returns the connection manager.
    ///
    /// # Returns
    ///
    /// An `Option` containing the `ConnectionManager` if the connection is successful, or `None` if
    /// it fails.
    ///
    /// # Errors
    ///
    /// Logs an error and returns `None` if the connection manager cannot be obtained.
    async fn lazy_connect(&self) -> Option<redis::aio::ConnectionManager> {
        if self.connection_manager.read().await.is_none() {
            let result = self.client.get_connection_manager().await;
            if let Err(e) = result {
                tracing::error!("Failed to get connection manager: {}", e);
                return None;
            }
            let mut cm = self.connection_manager.write().await;
            *cm = result.ok();
        }

        self.connection_manager.read().await.clone()
    }

    /// Generates a Redis key for a given node using the key prefix and the node's hash.
    ///
    /// # Parameters
    ///
    /// * `node` - The node for which the key is to be generated.
    ///
    /// # Returns
    ///
    /// A `String` representing the Redis key for the node.
    fn cache_key_for_node(&self, node: &Node<T>) -> String {
        format!("{}:{}", self.cache_key_prefix, node.id())
    }

    /// Generates a key for a given node to be persisted in Redis.
    fn persist_key_for_node(&self, node: &Node<T>) -> Result<String> {
        if let Some(key_fn) = self.persist_key_fn {
            key_fn(node)
        } else {
            let hash = node.id();
            Ok(format!("{}:{}", node.path.to_string_lossy(), hash))
        }
    }

    /// Resets the cache by deleting all keys with the specified prefix.
    /// This function is intended for testing purposes and is inefficient for production use.
    ///
    /// # Errors
    ///
    /// Panics if the keys cannot be retrieved or deleted.
    #[allow(dead_code)]
    async fn reset_cache(&self) {
        if let Some(mut cm) = self.lazy_connect().await {
            let keys: Vec<String> = redis::cmd("KEYS")
                .arg(format!("{}:*", self.cache_key_prefix))
                .query_async(&mut cm)
                .await
                .expect("Could not get keys");

            for key in &keys {
                let _: usize = redis::cmd("DEL")
                    .arg(key)
                    .query_async(&mut cm)
                    .await
                    .expect("Failed to reset cache");
            }
        }
    }

    /// Gets a node persisted in Redis using the GET command
    /// Takes a node and returns a Result<Option<String>>
    #[allow(dead_code)]
    async fn get_node(&self, node: &Node<T>) -> Result<Option<String>> {
        if let Some(mut cm) = self.lazy_connect().await {
            let key = self.persist_key_for_node(node)?;
            let result: Option<String> = redis::cmd("GET")
                .arg(key)
                .query_async(&mut cm)
                .await
                .context("Error getting from redis")?;
            Ok(result)
        } else {
            anyhow::bail!("Failed to connect to Redis")
        }
    }
}

impl<T: Chunk + Serialize> Redis<T> {
    /// Generates a value for a given node to be persisted in Redis.
    /// By default, the node is serialized as JSON.
    /// If a custom function is provided, it is used to generate the value.
    /// Otherwise, the node is serialized as JSON.
    fn persist_value_for_node(&self, node: &Node<T>) -> Result<String> {
        if let Some(value_fn) = self.persist_value_fn {
            value_fn(node)
        } else {
            Ok(serde_json::to_string(node)?)
        }
    }
}

// Redis CM does not implement debug
#[allow(clippy::missing_fields_in_debug)]
impl<T: Chunk> std::fmt::Debug for Redis<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Redis")
            .field("client", &self.client)
            .finish()
    }
}
