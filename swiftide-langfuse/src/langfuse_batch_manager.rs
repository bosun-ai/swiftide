use crate::apis::configuration::Configuration;
use crate::apis::ingestion_api::ingestion_batch;
use crate::models::{IngestionBatchRequest, IngestionEvent};
use crate::tracing_layer::{ObservationLayer, SpanTracker};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

const DEFAULT_LANGFUSE_URL: &str = "http://localhost:3000";

#[derive(Debug, Serialize, Deserialize)]
struct LangfuseIngestionResponse {
    successes: Vec<LangfuseIngestionSuccess>,
    errors: Vec<LangfuseIngestionError>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LangfuseIngestionSuccess {
    id: String,
    status: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct LangfuseIngestionError {
    id: String,
    status: i32,
    message: Option<String>,
    error: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct LangfuseBatchManager {
    config: Configuration,
    pub batch: Vec<IngestionEvent>,
}

impl LangfuseBatchManager {
    pub fn new(config: Configuration) -> Self {
        Self {
            config,
            batch: Vec::new(),
        }
    }

    // TODO: Graceful shutdown
    pub fn spawn_sender(manager: Arc<Mutex<Self>>) {
        const BATCH_INTERVAL: Duration = Duration::from_secs(5);

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(BATCH_INTERVAL).await;
                if let Err(e) = manager.lock().await.send_async().await {
                    tracing::error!(
                        error.msg = %e,
                        error.type = %std::any::type_name_of_val(&e),
                        "Failed to send batch to Langfuse"
                    );
                }
            }
        });
    }

    pub async fn send_async(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.batch.is_empty() {
            return Ok(());
        }

        let batch = std::mem::take(&mut self.batch);
        let mut payload = IngestionBatchRequest {
            batch,
            metadata: None, // Optional metadata can be added here if needed
        };

        let response = ingestion_batch(&self.config, &payload).await?;

        for error in &response.errors {
            // Any errors we log and ignore, no retry
            tracing::error!(
                id = %error.id,
                status = error.status,
                message = error.message.as_ref().unwrap_or(&None).as_deref().unwrap_or("No message"),
                error = ?error.error,
                "Partial failure in batch ingestion"
            );
        }

        if response.successes.is_empty() {
            tracing::warn!("All items in the batch failed, retrying all items");
            self.batch = std::mem::take(&mut payload.batch);
        }

        if response.successes.is_empty() && !response.errors.is_empty() {
            Err("Langfuse ingestion failed for all items".into())
        } else {
            Ok(())
        }
    }

    pub fn add_event(&mut self, event: IngestionEvent) {
        self.batch.push(event);
    }
}

pub fn create_langfuse_observer() -> Option<ObservationLayer> {
    let public_key = env::var("LANGFUSE_PUBLIC_KEY")
        .or_else(|_| env::var("LANGFUSE_INIT_PROJECT_PUBLIC_KEY"))
        .unwrap_or_default(); // Use empty string if not found

    let secret_key = env::var("LANGFUSE_SECRET_KEY")
        .or_else(|_| env::var("LANGFUSE_INIT_PROJECT_SECRET_KEY"))
        .unwrap_or_default(); // Use empty string if not found

    // Return None if either key is empty
    if public_key.is_empty() || secret_key.is_empty() {
        return None;
    }

    let base_url = env::var("LANGFUSE_URL").unwrap_or_else(|_| DEFAULT_LANGFUSE_URL.to_string());

    let config = Configuration {
        base_path: base_url.clone(),
        user_agent: Some("langfuse-rust-sdk".to_string()),
        client: Client::new(),
        basic_auth: Some((public_key.clone(), Some(secret_key.clone()))),
        ..Default::default()
    };

    let batch_manager = Arc::new(Mutex::new(LangfuseBatchManager::new(config)));

    if !cfg!(test) {
        LangfuseBatchManager::spawn_sender(batch_manager.clone());
    }

    Some(ObservationLayer {
        batch_manager,
        span_tracker: Arc::new(Mutex::new(SpanTracker::new())),
    })
}
