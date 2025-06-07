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
        self.create_topic_if_not_exists().await?;
        Ok(())
    }

    fn batch_size(&self) -> Option<usize> {
        None
    }

    async fn store(&self, node: Node) -> Result<Node> {
        self.producer()?
            .send(
                FutureRecord::to(&self.topic)
                    .payload(&self.persist_value_for_node(&node)?)
                    .key(&self.persist_key_for_node(&node)?),
                Duration::from_secs(0),
            )
            .await
            .map_err(|(e, _)| anyhow::anyhow!("Failed to send node: {:?}", e))?;
        Ok(node)
    }

    async fn batch_store(&self, nodes: Vec<Node>) -> IndexingStream {
        let producer = Arc::new(self.producer().expect("Failed to create producer"));
        let mut results = Vec::new();

        for node in nodes {
            let payload = self.persist_value_for_node(&node);
            let key = self.persist_key_for_node(&node);
            let send_result = match (payload, key) {
                (Ok(payload), Ok(key)) => {
                    producer
                        .send(
                            FutureRecord::to(&self.topic).payload(&payload).key(&key),
                            Duration::from_secs(0),
                        )
                        .await
                }
                _ => {
                    return vec![Err(anyhow::anyhow!(
                        "persist_value_for_node or persist_key_for_node failed"
                    ))]
                    .into();
                }
            };

            match send_result {
                Ok(_) => results.push(Ok(node)),
                Err((e, _)) => {
                    return vec![Err(anyhow::anyhow!("failed to send node: {:?}", e))].into();
                }
            }
        }

        IndexingStream::iter(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
