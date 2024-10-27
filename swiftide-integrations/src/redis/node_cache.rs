use anyhow::Result;
use async_trait::async_trait;

use swiftide_core::indexing::{Node, NodeCache};

use super::Redis;

#[allow(dependency_on_unit_never_type_fallback)]
#[async_trait]
impl NodeCache for Redis {
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
    #[tracing::instrument(skip_all, name = "node_cache.redis.get", fields(hit))]
    async fn get(&self, node: &Node) -> bool {
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
    #[tracing::instrument(skip_all, name = "node_cache.redis.get")]
    async fn set(&self, node: &Node) {
        if let Some(mut cm) = self.lazy_connect().await {
            let result: Result<(), redis::RedisError> = redis::cmd("SET")
                .arg(self.cache_key_for_node(node))
                .arg(1)
                .query_async(&mut cm)
                .await;

            if let Err(e) = result {
                tracing::error!("Failed to set node cache: {}", e);
            }
        }
    }

    async fn clear(&self) -> Result<()> {
        if self.cache_key_prefix.is_empty() {
            return Err(anyhow::anyhow!(
                "No cache key prefix set; not flushing cache"
            ));
        }

        if let Some(mut cm) = self.lazy_connect().await {
            redis::cmd("DEL")
                .arg(format!("{}*", self.cache_key_prefix))
                .query_async(&mut cm)
                .await?;

            Ok(())
        } else {
            anyhow::bail!("Failed to connect to Redis");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let node = Node::new("chunk");

        let before_cache = cache.get(&node).await;
        assert!(!before_cache);

        cache.set(&node).await;
        let after_cache = cache.get(&node).await;
        assert!(after_cache);
    }
}
