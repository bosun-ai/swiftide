use std::string::ToString;

use futures_util::{StreamExt as _, TryStreamExt as _};
use swiftide_core::{indexing::IndexingStream, indexing::Node, Loader};
use tokio::runtime::Handle;

use super::Fluvio;

impl Loader for Fluvio {
    #[tracing::instrument]
    fn into_stream(self) -> IndexingStream {
        let config = self.consumer_config_ext;

        let stream = tokio::task::block_in_place(|| {
            Handle::current().block_on(async {
                let client = fluvio::Fluvio::connect().await?;
                client.consumer_with_config(config).await
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
}
