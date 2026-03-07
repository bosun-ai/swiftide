//! Pipeline statistics for monitoring and observability
//!
//! This module provides statistics collection for pipelines, including:
//! - Number of nodes processed
//! - Number of errors
//! - Token usage per model
//! - Timing information
//!
//! The statistics can be accessed in real-time during pipeline execution
//! and provide insights into pipeline performance and costs.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Statistics for a single pipeline run
#[derive(Debug, Clone, Default)]
pub struct PipelineStats {
    /// Total number of nodes processed
    pub nodes_processed: u64,
    /// Total number of nodes that resulted in an error
    pub nodes_failed: u64,
    /// Token usage per model
    pub token_usage: HashMap<String, ModelUsage>,
    /// When the pipeline started
    pub started_at: Option<Instant>,
    /// When the pipeline completed
    pub completed_at: Option<Instant>,
    /// Number of transformations applied
    pub transformations_applied: u64,
    /// Number of nodes stored
    pub nodes_stored: u64,
}

/// Token usage for a specific model
#[derive(Debug, Clone, Default)]
pub struct ModelUsage {
    /// Number of prompt tokens used
    pub prompt_tokens: u64,
    /// Number of completion tokens used
    pub completion_tokens: u64,
    /// Total tokens used
    pub total_tokens: u64,
    /// Number of requests made
    pub request_count: u64,
}

impl ModelUsage {
    /// Add usage from a single request
    pub fn add_usage(&mut self, prompt: u64, completion: u64) {
        self.prompt_tokens += prompt;
        self.completion_tokens += completion;
        self.total_tokens += prompt + completion;
        self.request_count += 1;
    }
}

/// Thread-safe pipeline statistics collector
#[derive(Debug, Clone)]
pub struct StatsCollector {
    inner: Arc<StatsCollectorInner>,
}

#[derive(Debug)]
struct StatsCollectorInner {
    nodes_processed: AtomicU64,
    nodes_failed: AtomicU64,
    nodes_stored: AtomicU64,
    transformations_applied: AtomicU64,
    token_usage: Mutex<HashMap<String, ModelUsage>>,
    started_at: Mutex<Option<Instant>>,
    completed_at: Mutex<Option<Instant>>,
}

impl Default for StatsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl StatsCollector {
    /// Create a new stats collector
    pub fn new() -> Self {
        Self {
            inner: Arc::new(StatsCollectorInner {
                nodes_processed: AtomicU64::new(0),
                nodes_failed: AtomicU64::new(0),
                nodes_stored: AtomicU64::new(0),
                transformations_applied: AtomicU64::new(0),
                token_usage: Mutex::new(HashMap::new()),
                started_at: Mutex::new(None),
                completed_at: Mutex::new(None),
            }),
        }
    }

    /// Mark the pipeline as started
    ///
    /// # Panics
    ///
    /// Panics if the mutex is poisoned
    pub fn start(&self) {
        let mut started = self.inner.started_at.lock().unwrap();
        *started = Some(Instant::now());
    }

    /// Mark the pipeline as completed
    ///
    /// # Panics
    ///
    /// Panics if the mutex is poisoned
    pub fn complete(&self) {
        let mut completed = self.inner.completed_at.lock().unwrap();
        *completed = Some(Instant::now());
    }

    /// Increment the number of processed nodes
    pub fn increment_nodes_processed(&self, count: u64) {
        self.inner
            .nodes_processed
            .fetch_add(count, Ordering::Relaxed);
    }

    /// Increment the number of failed nodes
    pub fn increment_nodes_failed(&self, count: u64) {
        self.inner.nodes_failed.fetch_add(count, Ordering::Relaxed);
    }

    /// Increment the number of stored nodes
    pub fn increment_nodes_stored(&self, count: u64) {
        self.inner.nodes_stored.fetch_add(count, Ordering::Relaxed);
    }

    /// Increment the number of transformations applied
    pub fn increment_transformations(&self, count: u64) {
        self.inner
            .transformations_applied
            .fetch_add(count, Ordering::Relaxed);
    }

    /// Record token usage for a specific model
    ///
    /// # Panics
    ///
    /// Panics if the mutex is poisoned
    pub fn record_token_usage(&self, model: &str, prompt_tokens: u64, completion_tokens: u64) {
        let mut usage = self.inner.token_usage.lock().unwrap();
        usage
            .entry(model.to_string())
            .or_default()
            .add_usage(prompt_tokens, completion_tokens);
    }

    /// Get current statistics
    ///
    /// # Panics
    ///
    /// Panics if any mutex is poisoned
    pub fn get_stats(&self) -> PipelineStats {
        PipelineStats {
            nodes_processed: self.inner.nodes_processed.load(Ordering::Relaxed),
            nodes_failed: self.inner.nodes_failed.load(Ordering::Relaxed),
            nodes_stored: self.inner.nodes_stored.load(Ordering::Relaxed),
            transformations_applied: self.inner.transformations_applied.load(Ordering::Relaxed),
            token_usage: self.inner.token_usage.lock().unwrap().clone(),
            started_at: *self.inner.started_at.lock().unwrap(),
            completed_at: *self.inner.completed_at.lock().unwrap(),
        }
    }

    /// Get the elapsed time since the pipeline started
    ///
    /// # Panics
    ///
    /// Panics if the mutex is poisoned
    pub fn elapsed(&self) -> Option<std::time::Duration> {
        self.inner
            .started_at
            .lock()
            .unwrap()
            .map(|start| start.elapsed())
    }

    /// Check if the pipeline is still running
    ///
    /// # Panics
    ///
    /// Panics if any mutex is poisoned
    pub fn is_running(&self) -> bool {
        let started = self.inner.started_at.lock().unwrap();
        let completed = self.inner.completed_at.lock().unwrap();
        started.is_some() && completed.is_none()
    }
}

impl PipelineStats {
    /// Get the total number of tokens used across all models
    pub fn total_tokens(&self) -> u64 {
        self.token_usage.values().map(|u| u.total_tokens).sum()
    }

    /// Get the total number of requests made across all models
    pub fn total_requests(&self) -> u64 {
        self.token_usage.values().map(|u| u.request_count).sum()
    }

    /// Get the total duration of the pipeline run
    pub fn duration(&self) -> Option<std::time::Duration> {
        match (self.started_at, self.completed_at) {
            (Some(start), Some(end)) => Some(end.duration_since(start)),
            (Some(start), None) => Some(start.elapsed()),
            _ => None,
        }
    }

    /// Calculate the processing rate (nodes per second)
    #[allow(clippy::cast_precision_loss)]
    pub fn nodes_per_second(&self) -> Option<f64> {
        self.duration().map(|d| {
            let secs = d.as_secs_f64();
            if secs > 0.0 {
                self.nodes_processed as f64 / secs
            } else {
                0.0
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_collector() {
        let collector = StatsCollector::new();

        collector.start();
        collector.increment_nodes_processed(10);
        collector.increment_nodes_failed(2);
        collector.increment_nodes_stored(8);
        collector.increment_transformations(5);
        collector.record_token_usage("gpt-4", 100, 50);
        collector.record_token_usage("gpt-4", 200, 100);
        collector.complete();

        let stats = collector.get_stats();
        assert_eq!(stats.nodes_processed, 10);
        assert_eq!(stats.nodes_failed, 2);
        assert_eq!(stats.nodes_stored, 8);
        assert_eq!(stats.transformations_applied, 5);
        assert_eq!(stats.total_tokens(), 450);
        assert_eq!(stats.total_requests(), 2);
        assert!(stats.duration().is_some());
    }

    #[test]
    fn test_model_usage() {
        let mut usage = ModelUsage::default();
        usage.add_usage(100, 50);
        usage.add_usage(200, 100);

        assert_eq!(usage.prompt_tokens, 300);
        assert_eq!(usage.completion_tokens, 150);
        assert_eq!(usage.total_tokens, 450);
        assert_eq!(usage.request_count, 2);
    }
}
