//! Fluvio is a real-time streaming data transformation platform.
//!
//! This module provides a Fluvio loader for Swiftide and allows you to ingest
//! messages from Fluvio topics and use them for RAG

use derive_builder::Builder;

/// Re-export the fluvio config builder
pub use fluvio::consumer::{ConsumerConfigExt, ConsumerConfigExtBuilder};
use fluvio::FluvioConfig;

mod loader;

#[derive(Debug, Clone, Builder)]
#[builder(setter(into, strip_option))]
pub struct Fluvio {
    /// The Fluvio consumer configuration to use.
    consumer_config_ext: ConsumerConfigExt,

    #[builder(default, setter(custom))]
    fluvio_config: Option<FluvioConfig>,
}

impl Fluvio {
    pub fn from_consumer_config(config: impl Into<ConsumerConfigExt>) -> Fluvio {
        Fluvio {
            consumer_config_ext: config.into(),
            fluvio_config: None,
        }
    }

    pub fn builder() -> FluvioBuilder {
        FluvioBuilder::default()
    }
}

impl FluvioBuilder {
    pub fn fluvio_config(&mut self, config: &FluvioConfig) -> &mut Self {
        self.fluvio_config = Some(Some(config.to_owned()));

        self
    }
}
