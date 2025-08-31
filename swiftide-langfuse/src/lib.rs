//! Provides a Langfuse integration for Swiftide
//!
//! Agents and completion traits will report their input, output, and usage to langfuse.
//!
//! The `LangfuseLayer` needs to be set up like any other tracing layer.
//!
//! See the `default_tracing_layer` function for an example of how to set it up.
//!
//! Refer to the [Langfuse documentation](https://langfuse.com/docs/) for more details on how to setup Langfuse itself.
mod apis;
mod langfuse_batch_manager;
mod models;
mod tracing_layer;

use anyhow::Result;
use std::{env, sync::Arc};
use tokio::sync::Mutex;

const DEFAULT_LANGFUSE_URL: &str = "http://localhost:3000";

use reqwest::Client;

use crate::tracing_layer::SpanTracker;

pub use crate::apis::configuration::Configuration;
pub use crate::langfuse_batch_manager::LangfuseBatchManager;
pub use crate::tracing_layer::LangfuseLayer;

/// Creates a new langfuse tracing layer with values from environment variables.
///
/// Requires the following environment variables to be set:
/// LANGFUSE_PUBLIC_KEY
/// LANGFUSE_SECRET_KEY
///
/// Optionally, you can set LANGFUSE_URL to override the default Langfuse server URL.
pub fn default_tracing_layer() -> Result<LangfuseLayer> {
    let public_key = env::var("LANGFUSE_PUBLIC_KEY")
        .or_else(|_| env::var("LANGFUSE_INIT_PROJECT_PUBLIC_KEY"))
        .unwrap_or_default(); // Use empty string if not found

    let secret_key = env::var("LANGFUSE_SECRET_KEY")
        .or_else(|_| env::var("LANGFUSE_INIT_PROJECT_SECRET_KEY"))
        .unwrap_or_default(); // Use empty string if not found

    // Return None if either key is empty
    if public_key.is_empty() || secret_key.is_empty() {
        anyhow::bail!(
            "Public key or secret key not set. Please set LANGFUSE_PUBLIC_KEY and LANGFUSE_SECRET_KEY environment variables."
        );
    }

    let base_url = env::var("LANGFUSE_URL").unwrap_or_else(|_| DEFAULT_LANGFUSE_URL.to_string());

    let config = Configuration {
        base_path: base_url.clone(),
        user_agent: Some("swiftide".to_string()),
        client: Client::new(),
        basic_auth: Some((public_key.clone(), Some(secret_key.clone()))),
        ..Default::default()
    };

    let batch_manager = LangfuseBatchManager::new(config);

    batch_manager.clone().spawn();

    Ok(LangfuseLayer {
        batch_manager,
        span_tracker: Arc::new(Mutex::new(SpanTracker::new())),
    })
}

pub fn layer_from_config(config: Configuration) -> Result<LangfuseLayer> {
    let batch_manager = LangfuseBatchManager::new(config);

    batch_manager.clone().spawn();

    Ok(LangfuseLayer {
        batch_manager,
        span_tracker: Arc::new(Mutex::new(SpanTracker::new())),
    })
}
