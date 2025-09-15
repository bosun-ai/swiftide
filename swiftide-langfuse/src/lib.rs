//! Provides a Langfuse integration for Swiftide
//!
//! Agents and completion traits will report their input, output, and usage to langfuse.
//!
//! The `LangfuseLayer` needs to be set up like any other tracing layer.
//!
//! By default, it requires the LANGFUSE_PUBLIC_KEY and LANGFUSE_SECRET_KEY environment variables
//! to be set. You can also provide a custom Langfuse URL via the LANGFUSE_URL environment
//! variable.
//!
//! All `Langfuse` data is on the `debug` level. Make sure your tracing setup captures that level.
//!
//! # Example
//! ```
//! use swiftide::langfuse;
//!
//! // Assuming you have other layers
//! let mut layers = Vec::new();
//! layers.push(LangfuseLayer::default().with_filter(LevelFilter::DEBUG).boxed());
//!
//! let registry = tracing_subscriber::registry()
//!     .with(layers);
//!
//! registry.init();
//! ```
//!
//! For more advanced usage, refer to the `LangfuseLayer` documentation.
//!
//! Refer to the [Langfuse documentation](https://langfuse.com/docs/) for more details on how to setup Langfuse itself.
mod apis;
mod langfuse_batch_manager;
mod models;
mod tracing_layer;

const DEFAULT_LANGFUSE_URL: &str = "http://localhost:3000";

pub use crate::apis::configuration::Configuration;
pub use crate::langfuse_batch_manager::LangfuseBatchManager;
pub use crate::tracing_layer::LangfuseLayer;
