use crate::apis::configuration::Configuration;
use crate::apis::ingestion_api::ingestion_batch;
use crate::models::{IngestionBatchRequest, IngestionError, IngestionEvent};
use anyhow::Result;
use async_trait::async_trait;
use dyn_clone::DynClone;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;

/// How often a single event is sent before it is dropped on retryable failures.
const MAX_SEND_ATTEMPTS: u32 = 3;

#[derive(Debug, Default, Clone)]
pub struct LangfuseBatchManager {
    config: Arc<Configuration>,
    pub batch: Arc<Mutex<Vec<IngestionEvent>>>,
    /// Send attempts per event id, only tracked for events waiting to be retried.
    send_attempts: Arc<Mutex<HashMap<String, u32>>>,
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
            send_attempts: Arc::default(),

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
            tracing::error!(
                id = %error.id,
                status = error.status,
                retryable = is_retryable(error.status),
                message = error.message.as_ref().unwrap_or(&None).as_deref().unwrap_or("No message"),
                error = ?error.error,
                "Partial failure in batch ingestion"
            );
        }

        self.requeue_retryable(&mut payload.batch, &response.errors)
            .await;

        if response.successes.is_empty() && !response.errors.is_empty() {
            anyhow::bail!("Langfuse ingestion failed for all items");
        } else {
            Ok(())
        }
    }

    pub async fn add_event(&self, event: IngestionEvent) {
        self.batch.lock().await.push(event);
    }

    /// Puts events that failed with a retryable status back in front of the pending batch.
    ///
    /// Everything else in `sent` was either accepted or will never be accepted. Each event is
    /// sent at most `MAX_SEND_ATTEMPTS` times, so a persistently failing Langfuse cannot make
    /// the pending batch grow without bound.
    async fn requeue_retryable(&self, sent: &mut Vec<IngestionEvent>, errors: &[IngestionError]) {
        let retryable: HashSet<&str> = errors
            .iter()
            .filter(|error| is_retryable(error.status))
            .map(|error| error.id.as_str())
            .collect();
        let rejected = errors.len() - retryable.len();

        let mut exhausted = 0;
        let mut attempts = self.send_attempts.lock().await;
        sent.retain(|event| {
            let id = event.id();
            if !retryable.contains(id) {
                attempts.remove(id);
                return false;
            }
            let count = attempts.entry(id.to_owned()).or_insert(0);
            *count += 1;
            if *count < MAX_SEND_ATTEMPTS {
                return true;
            }
            attempts.remove(id);
            exhausted += 1;
            false
        });
        drop(attempts);

        if rejected + exhausted > 0 {
            tracing::warn!(
                rejected,
                exhausted,
                "Dropping Langfuse events that will not be retried"
            );
        }

        if !sent.is_empty() {
            tracing::warn!(count = sent.len(), "Retrying failed Langfuse events");
            self.batch.lock().await.splice(0..0, sent.drain(..));
        }
    }
}

/// Langfuse reports per event statuses; only throttling and server errors can succeed later.
fn is_retryable(status: i32) -> bool {
    status == 429 || (500..600).contains(&status)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TraceBody;
    use serde_json::{Value, json};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    /// Langfuse answering 207 with every event in the batch failing with `status`.
    async fn langfuse_failing_every_event_with(status: i32) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/public/ingestion"))
            .respond_with(move |request: &Request| {
                let body: Value = serde_json::from_slice(&request.body).unwrap();
                let errors: Vec<Value> = body["batch"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|event| json!({ "id": event["id"], "status": status }))
                    .collect();
                ResponseTemplate::new(207)
                    .set_body_json(json!({ "successes": [], "errors": errors }))
            })
            .mount(&server)
            .await;
        server
    }

    fn manager_for(server: &MockServer) -> LangfuseBatchManager {
        LangfuseBatchManager::new(Configuration {
            base_path: server.uri(),
            ..Default::default()
        })
    }

    fn trace_event() -> IngestionEvent {
        IngestionEvent::new_trace_create(TraceBody::default())
    }

    async fn pending_events(manager: &LangfuseBatchManager) -> usize {
        manager.batch.lock().await.len()
    }

    #[tokio::test]
    async fn drops_events_langfuse_rejects_as_client_errors() {
        let server = langfuse_failing_every_event_with(400).await;
        let manager = manager_for(&server);
        manager.add_event(trace_event()).await;
        manager.add_event(trace_event()).await;

        assert!(manager.send_async().await.is_err());

        assert_eq!(pending_events(&manager).await, 0);
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn retries_server_errors_a_bounded_number_of_times() {
        let server = langfuse_failing_every_event_with(500).await;
        let manager = manager_for(&server);
        manager.add_event(trace_event()).await;

        for _ in 0..MAX_SEND_ATTEMPTS + 2 {
            let _ = manager.send_async().await;
        }

        assert_eq!(pending_events(&manager).await, 0);
        assert!(manager.send_attempts.lock().await.is_empty());
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            MAX_SEND_ATTEMPTS as usize
        );
    }

    #[tokio::test]
    async fn pending_batch_stays_bounded_while_langfuse_keeps_failing() {
        let server = langfuse_failing_every_event_with(503).await;
        let manager = manager_for(&server);

        for _ in 0..20 {
            manager.add_event(trace_event()).await;
            let _ = manager.send_async().await;

            // Every event is retained for at most `MAX_SEND_ATTEMPTS - 1` failed flushes
            assert!(pending_events(&manager).await < MAX_SEND_ATTEMPTS as usize);
            assert!(manager.send_attempts.lock().await.len() < MAX_SEND_ATTEMPTS as usize);
        }
    }

    #[tokio::test]
    async fn empty_response_retries_nothing() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(207).set_body_json(json!({ "successes": [], "errors": [] })),
            )
            .mount(&server)
            .await;
        let manager = manager_for(&server);
        manager.add_event(trace_event()).await;

        manager.send_async().await.unwrap();

        assert_eq!(pending_events(&manager).await, 0);
    }
}
