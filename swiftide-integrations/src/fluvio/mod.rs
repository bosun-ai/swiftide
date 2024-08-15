//! Fluvio is a real-time streaming data transformation platform.
//!
//! This module provides a Fluvio loader for Swiftide and allows you to ingest
//! messages from Fluvio topics and use them for RAG

use derive_builder::Builder;
/// Re-export the fluvio config builder
pub use fluvio::consumer::{ConsumerConfigExt, ConsumerConfigExtBuilder};

mod loader;

#[derive(Debug, Clone, Builder)]
#[builder(setter(into, strip_option))]
pub struct Fluvio {
    /// The Fluvio consumer configuration to use.
    consumer_config_ext: ConsumerConfigExt,
}
