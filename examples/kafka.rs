//! # [Swiftide] Loading data from Kafka
//!
//! This example demonstrates how to index data from a Kafka topic and store the data in another
//! Kafka topic. Note that for it to work correctly you need to have kafka.
//!
//! The pipeline will:
//! - Load messages from a Kafka topic
//! - Embed the chunks in batches of 10
//! - Store the nodes in kafka
//!
//! [Swiftide]: https://github.com/bosun-ai/swiftide
//! [examples]: https://github.com/bosun-ai/swiftide/blob/master/examples

use swiftide::{
    indexing::{self, transformers::Embed},
    integrations::{
        fastembed::FastEmbed,
        kafka::{ClientConfig, Kafka},
    },
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    static LOADER_TOPIC: &str = "loader";
    static STORAGE_TOPIC: &str = "storage";

    let mut client_config = ClientConfig::new();
    client_config.set("bootstrap.servers", "localhost:9092");
    client_config.set("group.id", "group_id");
    client_config.set("auto.offset.reset", "earliest");

    let loader = Kafka::builder()
        .client_config(client_config.clone())
        .topic(LOADER_TOPIC)
        .build()
        .unwrap();

    let storage = Kafka::builder()
        .client_config(client_config)
        .topic(STORAGE_TOPIC)
        .build()
        .unwrap();

    indexing::Pipeline::from_loader(loader)
        .then_in_batch(Embed::new(FastEmbed::try_default().unwrap()).with_batch_size(10))
        .then_store_with(storage)
        .run()
        .await?;
    Ok(())
}
