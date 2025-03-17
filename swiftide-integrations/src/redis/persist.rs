use anyhow::{Context as _, Result};
use async_trait::async_trait;

use swiftide_core::{
    indexing::{IndexingStream, Node},
    Persist,
};

use super::Redis;

#[async_trait]
#[allow(dependency_on_unit_never_type_fallback)]
impl Persist for Redis {
    async fn setup(&self) -> Result<()> {
        Ok(())
    }

    fn batch_size(&self) -> Option<usize> {
        Some(self.batch_size)
    }

    /// Stores a node in Redis using the SET command.
    ///
    /// By default nodes are stored with the path and hash as key and the node serialized as JSON as
    /// value.
    ///
    /// You can customize the key and value used for storing nodes by setting the `persist_key_fn`
    /// and `persist_value_fn` fields.
    async fn store(&self, node: Node) -> Result<Node> {
        if let Some(mut cm) = self.lazy_connect().await {
            redis::cmd("SET")
                .arg(self.persist_key_for_node(&node)?)
                .arg(self.persist_value_for_node(&node)?)
                .query_async(&mut cm)
                .await
                .context("Error persisting to redis")?;

            Ok(node)
        } else {
            anyhow::bail!("Failed to connect to Redis")
        }
    }

    /// Stores a batch of nodes in Redis using the MSET command.
    ///
    /// By default nodes are stored with the path and hash as key and the node serialized as JSON as
    /// value.
    ///
    /// You can customize the key and value used for storing nodes by setting the `persist_key_fn`
    /// and `persist_value_fn` fields.
    async fn batch_store(&self, nodes: Vec<Node>) -> IndexingStream {
        // use mset for batch store
        if let Some(mut cm) = self.lazy_connect().await {
            let args = nodes
                .iter()
                .map(|node| -> Result<Vec<String>> {
                    let key = self.persist_key_for_node(node)?;
                    let value = self.persist_value_for_node(node)?;

                    Ok(vec![key, value])
                })
                .collect::<Result<Vec<_>>>();

            if args.is_err() {
                return vec![Err(args.unwrap_err())].into();
            }

            let args = args.unwrap();

            let result: Result<()> = redis::cmd("MSET")
                .arg(args)
                .query_async(&mut cm)
                .await
                .context("Error persisting to redis");

            if result.is_ok() {
                IndexingStream::iter(nodes.into_iter().map(Ok))
            } else {
                IndexingStream::iter([Err(result.unwrap_err())])
            }
        } else {
            IndexingStream::iter([Err(anyhow::anyhow!("Failed to connect to Redis"))])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::TryStreamExt;
    use testcontainers::{runners::AsyncRunner, ContainerAsync, GenericImage};

    async fn start_redis() -> ContainerAsync<GenericImage> {
        testcontainers::GenericImage::new("redis", "7.2.4")
            .with_exposed_port(6379.into())
            .with_wait_for(testcontainers::core::WaitFor::message_on_stdout(
                "Ready to accept connections",
            ))
            .start()
            .await
            .expect("Redis started")
    }

    #[test_log::test(tokio::test)]
    async fn test_redis_persist() {
        let redis_container = start_redis().await;

        let host = redis_container.get_host().await.unwrap();
        let port = redis_container.get_host_port_ipv4(6379).await.unwrap();
        let redis = Redis::try_build_from_url(format!("redis://{host}:{port}"))
            .unwrap()
            .build()
            .unwrap();

        let node = Node::new("chunk");

        redis.store(node.clone()).await.unwrap();
        let stored_node = serde_json::from_str(&redis.get_node(&node).await.unwrap().unwrap());

        assert_eq!(node, stored_node.unwrap());
    }

    // test batch store
    #[test_log::test(tokio::test)]
    async fn test_redis_batch_persist() {
        let redis_container = start_redis().await;
        let host = redis_container.get_host().await.unwrap();
        let port = redis_container.get_host_port_ipv4(6379).await.unwrap();
        let redis = Redis::try_build_from_url(format!("redis://{host}:{port}"))
            .unwrap()
            .batch_size(20)
            .build()
            .unwrap();
        let nodes = vec![Node::new("test"), Node::new("other")];

        let stream = redis.batch_store(nodes).await;
        let streamed_nodes: Vec<Node> = stream.try_collect().await.unwrap();

        assert_eq!(streamed_nodes.len(), 2);

        for node in streamed_nodes {
            let stored_node = serde_json::from_str(&redis.get_node(&node).await.unwrap().unwrap());
            assert_eq!(node, stored_node.unwrap());
        }
    }

    #[test_log::test(tokio::test)]
    async fn test_redis_custom_persist() {
        let redis_container = start_redis().await;
        let host = redis_container.get_host().await.unwrap();
        let port = redis_container.get_host_port_ipv4(6379).await.unwrap();
        let redis = Redis::try_build_from_url(format!("redis://{host}:{port}"))
            .unwrap()
            .persist_key_fn(|_node| Ok("test".to_string()))
            .persist_value_fn(|_node| Ok("hello world".to_string()))
            .build()
            .unwrap();
        let node = Node::default();

        redis.store(node.clone()).await.unwrap();
        let stored_node = redis.get_node(&node).await.unwrap();

        assert_eq!(stored_node.unwrap(), "hello world");
        assert_eq!(
            redis.persist_key_for_node(&node).unwrap(),
            "test".to_string()
        );
    }
}
