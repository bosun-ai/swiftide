use crate::apis::configuration::Configuration;
use crate::apis::ingestion_api::ingestion_batch;
use crate::models::{IngestionBatchRequest, IngestionEvent};
use anyhow::Result;
use async_trait::async_trait;
use dyn_clone::DynClone;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;

#[derive(Debug, Default, Clone)]
pub struct LangfuseBatchManager {
    config: Arc<Configuration>,
    pub batch: Arc<Mutex<Vec<IngestionEvent>>>,
    dropped: Arc<AtomicBool>,
}

#[async_trait]
pub trait BatchManagerTrait: Send + Sync + DynClone {
    async fn add_event(&self, event: IngestionEvent);
    async fn flush(&self) -> anyhow::Result<()>;
    fn boxed(&self) -> Box<dyn BatchManagerTrait + Send + Sync>;
}

dyn_clone::clone_trait_object!(BatchManagerTrait);

impl LangfuseBatchManager {
    pub fn new(config: Configuration) -> Self {
        Self {
            config: Arc::new(config),
            batch: Arc::new(Mutex::new(Vec::new())),

            // Locally track if the manager has been dropped to avoid spawning tasks after drop
            dropped: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn spawn(self) {
        if self.dropped.load(Ordering::Relaxed) {
            tracing::trace!("LangfuseBatchManager has been dropped, not spawning sender task");
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

    pub async fn flush(&self) -> Result<()> {
        let lock = self.batch.lock().await;
        if !lock.is_empty() {
            drop(lock);
            self.send_async().await?;
        }
        Ok(())
    }

    pub async fn send_async(&self) -> Result<()> {
        tracing::trace!("Sending batch to Langfuse");
        if self.dropped.load(Ordering::Relaxed) {
            tracing::error!("LangfuseBatchManager has been dropped, not sending batch");
            return Ok(());
        }
        let mut batch_guard = self.batch.lock().await;
        if batch_guard.is_empty() {
            return Ok(());
        }

        let batch = std::mem::take(&mut *batch_guard);
        let mut payload = IngestionBatchRequest {
            batch,
            metadata: None, // Optional metadata can be added here if needed
        };

        drop(batch_guard); // Release the lock before making the network call

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
            tracing::error!("All items in the batch failed, retrying all items");

            let mut batch_guard = self.batch.lock().await;
            batch_guard.append(&mut payload.batch);
        }

        if response.successes.is_empty() && !response.errors.is_empty() {
            anyhow::bail!("Langfuse ingestion failed for all items");
        } else {
            Ok(())
        }
    }

    pub async fn add_event(&self, event: IngestionEvent) {
        self.batch.lock().await.push(event);
    }
}

#[async_trait]
impl BatchManagerTrait for LangfuseBatchManager {
    async fn add_event(&self, event: IngestionEvent) {
        self.add_event(event).await;
    }

    async fn flush(&self) -> anyhow::Result<()> {
        self.flush().await
    }

    fn boxed(&self) -> Box<dyn BatchManagerTrait + Send + Sync> {
        Box::new(self.clone())
    }
}

impl Drop for LangfuseBatchManager {
    fn drop(&mut self) {
        if Arc::strong_count(&self.dropped) > 1 {
            // There are other references to this manager, don't flush yet
            return;
        }
        if self.dropped.swap(true, Ordering::SeqCst) {
            // Already dropped
            return;
        }
        let this = self.clone();

        tokio::task::spawn_blocking(move || {
            let handle = tokio::runtime::Handle::current();
            if let Err(e) = handle.block_on(async move { this.flush().await }) {
                tracing::error!("Error flushing LangfuseBatchManager on drop: {:?}", e);
            }
        });
    }
}
