use futures_util::{StreamExt as _, stream};
use rdkafka::{
    Message,
    consumer::{CommitMode, Consumer, StreamConsumer},
    message::BorrowedMessage,
};
use swiftide_core::{Loader, indexing::IndexingStream, indexing::Node};

use super::Kafka;

impl Loader for Kafka {
    #[tracing::instrument]
    fn into_stream(self) -> IndexingStream {
        let client_config = self.client_config;
        let topic = self.topic.clone();

        let consumer: StreamConsumer = client_config
            .create()
            .expect("Failed to create Kafka consumer");

        consumer
            .subscribe(&[&topic])
            .unwrap_or_else(|_| panic!("Failed to subscribe to topic: {topic}"));

        let swiftide_stream = stream::unfold(consumer, |consumer| async move {
            loop {
                match consumer.recv().await {
                    Ok(message) => {
                        // only handle Some(Ok(s))
                        if let Some(Ok(payload)) = message.payload_view::<str>() {
                            tracing::debug!("Received message: {}", payload);
                            let mut node = Node::new(payload);
                            msg_metadata(&mut node, &message);
                            tracing::debug!("Node: {:?}", node);

                            // manually commit offset (if needed)
                            if let Err(e) = consumer.commit_message(&message, CommitMode::Async) {
                                tracing::warn!("Failed to commit offset: {:?}", e);
                            }

                            return Some((Ok(node), consumer));
                        }
                        tracing::debug!("Skipping message with invalid payload");
                    }
                    Err(e) => return Some((Err(anyhow::Error::from(e)), consumer)),
                }
            }
        });

        swiftide_stream.boxed().into()
    }

    fn into_stream_boxed(self: Box<Self>) -> IndexingStream {
        (*self).into_stream()
    }
}

fn msg_metadata(node: &mut Node, message: &BorrowedMessage) {
    // Add Kafka-specific metadata
    node.metadata
        .insert("kafka_topic", message.topic().to_string());

    node.metadata
        .insert("kafka_partition", message.partition().to_string());
    node.metadata
        .insert("kafka_offset", message.offset().to_string());

    // Add timestamp if present
    if let Some(timestamp) = message.timestamp().to_millis() {
        node.metadata
            .insert("kafka_timestamp", timestamp.to_string());
    }

    // Add key if present
    if let Some(Ok(key)) = message.key_view::<str>() {
        node.metadata.insert("kafka_key", key.to_string());
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::kafka::Kafka;
    use anyhow::Result;
    use futures_util::TryStreamExt;
    use rdkafka::{
        ClientConfig,
        admin::{AdminClient, AdminOptions, NewTopic, TopicReplication},
        client::DefaultClientContext,
        producer::{FutureProducer, FutureRecord, Producer},
    };
    use testcontainers::{ContainerAsync, runners::AsyncRunner};
    use testcontainers_modules::kafka::apache::{self};

    struct KafkaBroker {
        _broker: ContainerAsync<apache::Kafka>,
        partitions: i32,
        replicas: i32,
        client_config: ClientConfig,
    }

    impl KafkaBroker {
        pub async fn start() -> Result<Self> {
            static PARTITIONS: i32 = 1;
            static REPLICAS: i32 = 1;

            let kafka_node = apache::Kafka::default().start().await?;
            let bootstrap_servers = format!(
                "127.0.0.1:{}",
                kafka_node.get_host_port_ipv4(apache::KAFKA_PORT).await?
            );

            let mut client_config = ClientConfig::new();
            client_config.set("bootstrap.servers", &bootstrap_servers);
            client_config.set("group.id", "group_id");
            client_config.set("auto.offset.reset", "earliest");

            let broker = KafkaBroker {
                _broker: kafka_node,
                client_config,
                partitions: PARTITIONS,
                replicas: REPLICAS,
            };

            Ok(broker)
        }

        pub async fn create_topic(&self, topic: impl Into<String>) -> Result<()> {
            let admin = self.admin_client();
            admin
                .create_topics(
                    &[NewTopic {
                        name: topic.into().as_str(),
                        num_partitions: self.partitions,
                        replication: TopicReplication::Fixed(self.replicas),
                        config: vec![],
                    }],
                    &AdminOptions::default(),
                )
                .await
                .expect("topic creation failed");
            Ok(())
        }

        fn admin_client(&self) -> AdminClient<DefaultClientContext> {
            self.client_config.create().unwrap()
        }

        fn producer(&self) -> FutureProducer {
            self.client_config.create().unwrap()
        }
    }

    #[test_log::test(tokio::test(flavor = "multi_thread"))]
    async fn test_kafka_loader() {
        static TOPIC_NAME: &str = "topic";
        let kafka_broker = KafkaBroker::start().await.unwrap();
        kafka_broker.create_topic(TOPIC_NAME).await.unwrap();

        let producer = kafka_broker.producer();
        producer
            .send(
                FutureRecord::to(TOPIC_NAME).payload("payload").key("key"),
                Duration::from_secs(0),
            )
            .await
            .unwrap();
        producer.flush(Duration::from_secs(0)).unwrap();

        let loader = Kafka::builder()
            .client_config(kafka_broker.client_config.clone())
            .topic(TOPIC_NAME)
            .build()
            .unwrap();

        let node: Node = loader.into_stream().try_next().await.unwrap().unwrap();
        assert_eq!(node.chunk, "payload");
    }
}
