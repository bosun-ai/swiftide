use std::string::ToString;

use anyhow::Context as _;
use futures_util::{StreamExt as _, TryStreamExt as _};
use swiftide_core::{indexing::IndexingStream, indexing::Node, Loader};
use tokio::runtime::Handle;

use super::Fluvio;

impl Loader for Fluvio {
    #[tracing::instrument]
    fn into_stream(self) -> IndexingStream {
        let fluvio_config = self.fluvio_config;
        let consumer_config = self.consumer_config_ext;

        let stream = tokio::task::block_in_place(|| {
            Handle::current().block_on(async {
                let client = if let Some(fluvio_config) = &fluvio_config {
                    fluvio::Fluvio::connect_with_config(fluvio_config).await
                } else {
                    fluvio::Fluvio::connect().await
                }
                .context(format!("Failed to connect to Fluvio {fluvio_config:?}"))?;
                client.consumer_with_config(consumer_config).await
            })
        })
        .expect("Failed to connect to Fluvio");

        let swiftide_stream = stream
            .map_ok(|f| {
                let mut node = Node::new(f.get_value().to_string());
                node.metadata
                    .insert("fluvio_key", f.get_key().map(ToString::to_string));

                node
            })
            .map_err(anyhow::Error::from);

        swiftide_stream.boxed().into()
    }

    fn into_stream_boxed(self: Box<Self>) -> IndexingStream {
        self.into_stream()
    }
}

#[cfg(test)]
mod tests {
    use std::pin::Pin;

    use super::*;
    use anyhow::Result;
    use fluvio::{
        consumer::ConsumerConfigExt,
        metadata::{customspu::CustomSpuSpec, topic::TopicSpec},
        RecordKey,
    };
    use flv_util::socket_helpers::ServerAddress;
    use futures_util::TryStreamExt;
    use regex::Regex;
    use testcontainers::{runners::AsyncRunner, ContainerAsync, GenericImage, ImageExt};
    use tokio::io::{AsyncBufRead, AsyncBufReadExt};

    // NOTE: Move to test-utils / upstream to testcontainers if needed elsewhere
    struct FluvioCluster {
        sc: ContainerAsync<GenericImage>,
        spu: ContainerAsync<GenericImage>,

        partitions: u32,
        replicas: u32,
        port: u16,
        host_spu_port: u16,
        client: fluvio::Fluvio,
    }

    impl FluvioCluster {
        // Starts a fluvio cluster and connects the spu to the sc
        pub async fn start() -> Result<FluvioCluster> {
            static SC_PORT: u16 = 9003;
            static SPU_PORT1: u16 = 9010;
            static SPU_PORT2: u16 = 9011;
            static NETWORK_NAME: &str = "fluvio";
            static PARTITIONS: u32 = 1;
            static REPLICAS: u32 = 1;

            let sc = GenericImage::new("infinyon/fluvio", "latest")
                .with_exposed_port(SC_PORT.into())
                .with_wait_for(testcontainers::core::WaitFor::message_on_stdout(
                    "started successfully",
                ))
                .with_wait_for(testcontainers::core::WaitFor::seconds(1))
                .with_network(NETWORK_NAME)
                .with_container_name("sc")
                .with_cmd("./fluvio-run sc --local /fluvio/metadata".split(' '))
                .with_env_var("RUST_LOG", "info")
                .start()
                .await?;

            let spu = GenericImage::new("infinyon/fluvio", "latest")
                .with_exposed_port(SPU_PORT1.into())
                .with_wait_for(testcontainers::core::WaitFor::message_on_stdout(
                    "started successfully",
                ))
                    .with_wait_for(testcontainers::core::WaitFor::seconds(1))
                .with_network(NETWORK_NAME)
                .with_container_name("spu")
                .with_cmd(format!("./fluvio-run spu -i 5001 -p spu:{SPU_PORT1} -v spu:{SPU_PORT2} --sc-addr sc:9004 --log-base-dir /fluvio/data").split(' '))
                .with_env_var("RUST_LOG", "info")
                .start()
                .await?;

            let host_spu_port_1 = spu.get_host_port_ipv4(SPU_PORT1).await?;
            let sc_host_port = sc.get_host_port_ipv4(SC_PORT).await?;
            let endpoint = format!("127.0.0.1:{sc_host_port}");
            let config = fluvio::FluvioConfig::new(&endpoint);
            let client = fluvio::Fluvio::connect_with_config(&config).await?;

            let cluster = FluvioCluster {
                sc,
                spu,
                port: sc_host_port,
                host_spu_port: host_spu_port_1,
                client,
                replicas: REPLICAS,
                partitions: PARTITIONS,
            };

            cluster.connect_spu_to_sc().await;

            Ok(cluster)
        }

        async fn connect_spu_to_sc(&self) {
            let admin = self.client().admin().await;

            let spu_spec = CustomSpuSpec {
                id: 5001,
                public_endpoint: ServerAddress::try_from(format!("0.0.0.0:{}", self.host_spu_port))
                    .unwrap()
                    .into(),
                private_endpoint: ServerAddress::try_from(format!("spu:{}", 9011))
                    .unwrap()
                    .into(),
                rack: None,
                public_endpoint_local: None,
            };

            admin
                .create("SPU".to_string(), false, spu_spec)
                .await
                .unwrap();
        }

        pub fn forward_logs_to_tracing(&self) {
            Self::log_stdout(self.sc.stdout(true));
            Self::log_stderr(self.sc.stderr(true));

            Self::log_stdout(self.spu.stdout(true));
            Self::log_stderr(self.spu.stderr(true));
        }

        pub fn client(&self) -> &fluvio::Fluvio {
            &self.client
        }

        pub async fn create_topic(&self, topic_name: impl Into<String>) -> Result<()> {
            let admin = self.client().admin().await;
            let topic_spec = TopicSpec::new_computed(self.partitions, self.replicas, None);

            admin.create(topic_name.into(), false, topic_spec).await
        }

        fn log_stdout(reader: Pin<Box<dyn AsyncBufRead + Send>>) {
            let regex = Self::ansii_regex();
            tokio::spawn(async move {
                let mut lines = reader.lines();
                while let Some(line) = lines.next_line().await.unwrap() {
                    let line = regex.replace_all(&line, "").to_string();
                    tracing::info!(line);
                }
            });
        }

        fn log_stderr(reader: Pin<Box<dyn AsyncBufRead + Send>>) {
            let regex = Self::ansii_regex();
            tokio::spawn(async move {
                let mut lines = reader.lines();
                while let Some(line) = lines.next_line().await.unwrap() {
                    let line = regex.replace_all(&line, "").to_string();
                    tracing::error!(line);
                }
            });
        }

        fn ansii_regex() -> Regex {
            regex::Regex::new(r"\x1b\[([\x30-\x3f]*[\x20-\x2f]*[\x40-\x7e])").unwrap()
        }

        pub fn endpoint(&self) -> String {
            format!("127.0.0.1:{}", self.port)
        }
    }

    #[test_log::test(tokio::test(flavor = "multi_thread"))]
    async fn test_fluvio_loader() {
        static TOPIC_NAME: &str = "hello-rust";
        static PARTITION_NUM: u32 = 0;

        let fluvio_cluster = FluvioCluster::start()
            .await
            .expect("Failed to start Fluvio cluster");

        fluvio_cluster.forward_logs_to_tracing();
        fluvio_cluster.create_topic(TOPIC_NAME).await.unwrap();

        let client = fluvio_cluster.client();

        let producer = client.topic_producer(TOPIC_NAME).await.unwrap();
        producer
            .send(RecordKey::NULL, "Hello fluvio")
            .await
            .unwrap();
        producer.flush().await.unwrap();

        // Consume the topic with the loader
        let config = fluvio::FluvioConfig::new(fluvio_cluster.endpoint());
        let loader = Fluvio::builder()
            .fluvio_config(&config)
            .consumer_config_ext(
                ConsumerConfigExt::builder()
                    .topic(TOPIC_NAME)
                    .partition(PARTITION_NUM)
                    .offset_start(fluvio::Offset::from_end(1))
                    .build()
                    .unwrap(),
            )
            .build()
            .unwrap();

        let node: Node = loader.into_stream().try_next().await.unwrap().unwrap();

        assert_eq!(node.chunk, "Hello fluvio");
    }
}
