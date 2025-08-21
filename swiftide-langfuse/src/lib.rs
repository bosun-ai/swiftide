mod apis;
mod langfuse_batch_manager;
mod models;
mod tracing_layer;

use anyhow::Result;
use std::{env, sync::Arc};
use tokio::sync::Mutex;

const DEFAULT_LANGFUSE_URL: &str = "http://localhost:3000";

use reqwest::Client;

use crate::{
    apis::configuration::Configuration,
    langfuse_batch_manager::LangfuseBatchManager,
    tracing_layer::{LangfuseLayer, SpanTracker},
};

pub fn tracing_layer() -> Result<LangfuseLayer> {
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
        user_agent: Some("langfuse-rust-sdk".to_string()),
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
