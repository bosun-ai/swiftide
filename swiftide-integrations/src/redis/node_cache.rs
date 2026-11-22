use anyhow::Result;
use async_trait::async_trait;

use swiftide_core::indexing::{Chunk, Node, NodeCache};

use super::Redis;

#[allow(dependency_on_unit_never_type_fallback)]
#[async_trait]
impl<T: Chunk> NodeCache for Redis<T> {
    type Input = T;
    /// Checks if a node is present in the cache.
    ///
    /// # Parameters
    ///
    /// * `node` - The node to be checked in the cache.
    ///
    /// # Returns
    ///
    /// `true` if the node is present in the cache, `false` otherwise.
    ///
    /// # Errors
    ///
    /// Logs an error and returns `false` if the cache check fails.
    #[tracing::instrument(skip_all, fields(hit), level = "trace")]
    async fn get(&self, node: &Node<T>) -> bool {
        let cache_result = if let Some(mut cm) = self.lazy_connect().await {
            let result = redis::cmd("EXISTS")
                .arg(self.cache_key_for_node(node))
                .query_async(&mut cm)
                .await;

            match result {
                Ok(1) => true,
                Ok(0) => false,
                Err(e) => {
                    tracing::error!("Failed to check node cache: {}", e);
                    false
                }
                _ => {
                    tracing::error!("Unexpected response from redis");
                    false
                }
            }
        } else {
            false
        };

        tracing::Span::current().record("hit", cache_result);

        cache_result
    }

    /// Sets a node in the cache.
    ///
    /// # Parameters
    ///
    /// * `node` - The node to be set in the cache.
    ///
    /// # Errors
    ///
    /// Logs an error if the node cannot be set in the cache.
    #[tracing::instrument(skip_all, level = "trace")]
    async fn set(&self, node: &Node<T>) {
        self.set_by_id(node.parent_id().unwrap_or_else(|| node.id()))
            .await;
    }

    /// Marks the node identified by `id` as cached.
    ///
    /// # Errors
    ///
    /// Logs an error if the node cannot be set in the cache.
    #[tracing::instrument(skip_all, level = "trace")]
    async fn set_by_id(&self, id: uuid::Uuid) {
        if let Some(mut cm) = self.lazy_connect().await {
            let result: Result<(), redis::RedisError> = redis::cmd("SET")
                .arg(self.cache_key_for_id(id))
                .arg(1)
                .query_async(&mut cm)
                .await;

            if let Err(e) = result {
                tracing::error!("Failed to set node cache: {}", e);
            }
        }
    }

    /// Removes cached node IDs in this namespace while preserving message history.
    async fn clear(&self) -> Result<()> {
        if self.cache_key_prefix.is_empty() {
            return Err(anyhow::anyhow!(
                "No cache key prefix set; not flushing cache"
            ));
        }

        if let Some(mut cm) = self.lazy_connect().await {
            let prefix = format!("{}:", self.cache_key_prefix);
            let mut pattern = String::new();
            for ch in prefix.chars() {
                if matches!(ch, '\\' | '*' | '?' | '[' | ']') {
                    pattern.push('\\');
                }
                pattern.push(ch);
            }
            pattern.push('*');

            let mut cursor = 0_u64;
            loop {
                let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                    .arg(cursor)
                    .arg("MATCH")
                    .arg(&pattern)
                    .arg("COUNT")
                    .arg(100)
                    .query_async(&mut cm)
                    .await?;
                // Node cache keys are strings (`SET`) ending in a canonical UUID. Preserve
                // message history, including a UUID-keyed list owned by another instance,
                // and nested namespaces.
                let keys: Vec<_> = keys
                    .into_iter()
                    .filter(|key| {
                        key.as_str() != self.message_history_key.as_str()
                            && key.strip_prefix(&prefix).is_some_and(|id| {
                                uuid::Uuid::parse_str(id)
                                    .is_ok_and(|uuid| uuid.hyphenated().to_string() == id)
                            })
                    })
                    .collect();
                if !keys.is_empty() {
                    // History is a list (`RPUSH`). One TYPE pipeline per page, then one
                    // UNLINK of the string keys, rather than one call per key.
                    let mut types = redis::pipe();
                    for key in &keys {
                        types.cmd("TYPE").arg(key);
                    }
                    let types: Vec<String> = types.query_async(&mut cm).await?;
                    let keys: Vec<_> = keys
                        .into_iter()
                        .zip(types)
                        .filter_map(|(key, kind)| (kind == "string").then_some(key))
                        .collect();
                    if !keys.is_empty() {
                        redis::cmd("UNLINK")
                            .arg(&keys)
                            .query_async::<usize>(&mut cm)
                            .await?;
                    }
                }
                cursor = next_cursor;
                if cursor == 0 {
                    break;
                }
            }

            Ok(())
        } else {
            anyhow::bail!("Failed to connect to Redis");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use swiftide_core::indexing::TextNode;
    use testcontainers::runners::AsyncRunner;

    /// Tests the `RedisNodeCache` implementation.
    #[test_log::test(tokio::test)]
    async fn test_redis_cache() {
        let redis = testcontainers::GenericImage::new("redis", "7.2.4")
            .with_exposed_port(6379.into())
            .with_wait_for(testcontainers::core::WaitFor::message_on_stdout(
                "Ready to accept connections",
            ))
            .start()
            .await
            .expect("Redis started");

        let host = redis.get_host().await.unwrap();
        let port = redis.get_host_port_ipv4(6379).await.unwrap();
        let cache = Redis::try_from_url(format!("redis://{host}:{port}"), "test")
            .expect("Could not build redis client");
        cache.reset_cache().await;

        let node = TextNode::new("chunk");

        let before_cache = cache.get(&node).await;
        assert!(!before_cache);

        cache.set(&node).await;
        let after_cache = cache.get(&node).await;
        assert!(after_cache);
    }

    #[test_log::test(tokio::test)]
    async fn test_clear_only_removes_cached_nodes() {
        use swiftide_core::{MessageHistory, chat_completion::ChatMessage};

        let redis = testcontainers::GenericImage::new("redis", "7.2.4")
            .with_exposed_port(6379.into())
            .with_wait_for(testcontainers::core::WaitFor::message_on_stdout(
                "Ready to accept connections",
            ))
            .start()
            .await
            .expect("Redis started");
        let host = redis.get_host().await.unwrap();
        let port = redis.get_host_port_ipv4(6379).await.unwrap();
        let url = format!("redis://{host}:{port}");
        // Glob metacharacters in a namespace must be treated literally.
        let cache = Redis::try_from_url(&url, "test[1]*").unwrap();
        let neighbor = Redis::try_from_url(&url, "test[1]*:neighbor").unwrap();
        let glob_neighbor = Redis::try_from_url(&url, "test1-other").unwrap();
        // A UUID-suffixed history key inside the namespace must survive.
        let history_uuid_key = format!("test[1]*:{}", uuid::Uuid::nil());
        let mut history_cache = Redis::try_from_url(&url, "test[1]*").unwrap();
        history_cache.with_message_history_key(history_uuid_key);
        let nodes: Vec<_> = (0..256)
            .map(|index| TextNode::new(format!("chunk {index}")))
            .collect();
        for node in &nodes {
            cache.set(node).await;
            assert!(cache.get(node).await);
        }
        neighbor.set(&nodes[0]).await;
        glob_neighbor.set(&nodes[0]).await;
        let message = ChatMessage::new_system("Keep this history");
        cache.push_owned(message.clone()).await.unwrap();
        let history_message = ChatMessage::new_system("Keep this UUID-keyed history");
        history_cache
            .push_owned(history_message.clone())
            .await
            .unwrap();

        // Clear through the instance that owns the UUID-shaped history key: it is only
        // protected from its own `clear`, and must survive it.
        history_cache.clear().await.unwrap();

        for node in &nodes {
            assert!(!cache.get(node).await, "clear must remove cached nodes");
        }
        assert!(neighbor.get(&nodes[0]).await);
        assert!(glob_neighbor.get(&nodes[0]).await);
        assert_eq!(
            history_cache.history().await.unwrap(),
            vec![history_message.clone()]
        );
        assert_eq!(cache.history().await.unwrap(), vec![message.clone()]);
        cache.clear().await.unwrap();
        assert_eq!(cache.history().await.unwrap(), vec![message]);
        // The second clear does not own this UUID-keyed history, so the name filter
        // alone would unlink it. It must still be there.
        assert_eq!(
            history_cache.history().await.unwrap(),
            vec![history_message]
        );
    }
}
