use crate::apis::configuration::Configuration;
use crate::apis::ingestion_api::ingestion_batch;
use crate::models::{IngestionBatchRequest, IngestionEvent};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

#[derive(Debug, Default, Clone)]
pub struct LangfuseBatchManager {
    config: Arc<Configuration>,
    pub batch: Arc<Mutex<Vec<IngestionEvent>>>,
    dropped: bool,
}

impl LangfuseBatchManager {
    pub fn new(config: Configuration) -> Self {
        Self {
            config: Arc::new(config),
            batch: Arc::new(Mutex::new(Vec::new())),
            dropped: false,
        }
    }

    pub fn spawn(self) {
        if self.dropped {
            tracing::warn!("LangfuseBatchManager has been dropped, not spawning sender task");
            return;
        }

        const BATCH_INTERVAL: Duration = Duration::from_secs(5);

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(BATCH_INTERVAL).await;
                if let Err(e) = self.send_async().await {
                    tracing::error!(
                        error.msg = %e,
                        error.type = %std::any::type_name_of_val(&e),
                        "Failed to send batch to Langfuse"
                    );
                }
            }
        });
    }

    pub async fn flush(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.batch.lock().await.is_empty() {
            self.send_async().await?;
        }
        Ok(())
    }

    pub async fn send_async(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut batch_guard = self.batch.lock().await;
        if batch_guard.is_empty() {
            return Ok(());
        }

        let batch = std::mem::take(&mut *batch_guard);
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
            *batch_guard = std::mem::take(&mut payload.batch);
        }

        if response.successes.is_empty() && !response.errors.is_empty() {
            Err("Langfuse ingestion failed for all items".into())
        } else {
            Ok(())
        }
    }

    pub async fn add_event(&self, event: IngestionEvent) {
        self.batch.lock().await.push(event);
    }
}

impl Drop for LangfuseBatchManager {
    fn drop(&mut self) {
        let mut this = std::mem::take(self);
        self.dropped = true;

        tokio::task::spawn_blocking(move || {
            let handle = tokio::runtime::Handle::current();
            handle.block_on(async move { this.flush().await })
        });
    }
}
