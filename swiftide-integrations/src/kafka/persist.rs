use std::{sync::Arc, time::Duration};

use anyhow::Result;
use async_trait::async_trait;

use rdkafka::producer::FutureRecord;
use swiftide_core::{
    Persist,
    indexing::{IndexingStream, Node},
};

use super::Kafka;

#[async_trait]
impl Persist for Kafka {
    async fn setup(&self) -> Result<()> {
        if self.topic_exists()? {
            return Ok(());
        }
        if !self.create_topic_if_not_exists {
            return Err(anyhow::anyhow!("Topic {} does not exist", self.topic));
        }
        self.create_topic().await?;
        Ok(())
    }

    fn batch_size(&self) -> Option<usize> {
        Some(self.batch_size)
    }

    async fn store(&self, node: Node) -> Result<Node> {
        let (key, payload) = self.node_to_key_payload(&node)?;
        self.producer()?
            .send(
                FutureRecord::to(&self.topic).key(&key).payload(&payload),
                Duration::from_secs(0),
            )
            .await
            .map_err(|(e, _)| anyhow::anyhow!("Failed to send node: {:?}", e))?;
        Ok(node)
    }

    async fn batch_store(&self, nodes: Vec<Node>) -> IndexingStream {
        let producer = Arc::new(self.producer().expect("Failed to create producer"));

        for node in &nodes {
            match self.node_to_key_payload(node) {
                Ok((key, payload)) => {
                    if let Err(e) = producer
                        .send(
                            FutureRecord::to(&self.topic).payload(&payload).key(&key),
                            Duration::from_secs(0),
                        )
                        .await
                    {
                        return vec![Err(anyhow::anyhow!("failed to send node: {:?}", e))].into();
                    }
                }
                Err(e) => {
                    return vec![Err(e)].into();
                }
            }
        }

        IndexingStream::iter(nodes.into_iter().map(Ok))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::TryStreamExt;
    use rdkafka::ClientConfig;
    use testcontainers::runners::AsyncRunner;
    use testcontainers_modules::kafka::apache::{self};

    #[test_log::test(tokio::test)]
    async fn test_kafka_persist() {
        static TOPIC_NAME: &str = "topic";

        let kafka_node = apache::Kafka::default()
            .start()
            .await
            .expect("failed to start kafka");
        let bootstrap_servers = format!(
            "127.0.0.1:{}",
            kafka_node
                .get_host_port_ipv4(apache::KAFKA_PORT)
                .await
                .expect("failed to get kafka port")
        );

        let mut client_config = ClientConfig::new();
        client_config.set("bootstrap.servers", &bootstrap_servers);
        let storage = Kafka::builder()
            .client_config(client_config)
            .topic(TOPIC_NAME)
            .build()
            .unwrap();

        let node = Node::new("chunk");

        storage.setup().await.unwrap();
        storage.store(node.clone()).await.unwrap();
    }

    #[test_log::test(tokio::test)]
    async fn test_kafka_batch_persist() {
        static TOPIC_NAME: &str = "topic";

        let kafka_node = apache::Kafka::default()
            .start()
            .await
            .expect("failed to start kafka");
        let bootstrap_servers = format!(
            "127.0.0.1:{}",
            kafka_node
                .get_host_port_ipv4(apache::KAFKA_PORT)
                .await
                .expect("failed to get kafka port")
        );

        let mut client_config = ClientConfig::new();
        client_config.set("bootstrap.servers", &bootstrap_servers);
        let storage = Kafka::builder()
            .client_config(client_config)
            .topic(TOPIC_NAME)
            .create_topic_if_not_exists(true)
            .batch_size(2usize)
            .build()
            .unwrap();

        let nodes = vec![Node::default(); 6];

        storage.setup().await.unwrap();

        let stream = storage.batch_store(nodes.clone()).await;

        let result: Vec<Node> = stream.try_collect().await.unwrap();

        assert_eq!(result.len(), 6);
        assert_eq!(result[0], nodes[0]);
        assert_eq!(result[1], nodes[1]);
        assert_eq!(result[2], nodes[2]);
        assert_eq!(result[3], nodes[3]);
        assert_eq!(result[4], nodes[4]);
        assert_eq!(result[5], nodes[5]);
    }
}
