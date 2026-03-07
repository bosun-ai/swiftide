//! Pipeline statistics collection
//!
//! This module provides comprehensive monitoring and observability for pipelines,
//! including node counts, token usage, and timing information.
//!
//! # Example
//!
//! ```rust,ignore
//! use swiftide::indexing::Pipeline;
//!
//! let pipeline = Pipeline::from_loader(loader)
//!     .then(transformer)
//!     .store(storage);
//!
//! // Run pipeline
//! pipeline.run().await?;
//!
//! // Get statistics
//! let stats = pipeline.stats();
//! println!("Processed {} nodes in {:?}", stats.nodes_processed, stats.duration());
//! ```

use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

/// Statistics for a single model's usage
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelUsage {
    /// Number of prompt tokens used
    pub prompt_tokens: u64,
    /// Number of completion tokens used
    pub completion_tokens: u64,
    /// Total tokens used (prompt + completion)
    pub total_tokens: u64,
    /// Number of requests made to this model
    pub request_count: u64,
}

impl ModelUsage {
    /// Creates a new `ModelUsage` with zero counts
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records token usage for a single request
    pub fn record(&mut self, prompt_tokens: u64, completion_tokens: u64) {
        self.prompt_tokens += prompt_tokens;
        self.completion_tokens += completion_tokens;
        self.total_tokens += prompt_tokens + completion_tokens;
        self.request_count += 1;
    }
}

/// A snapshot of pipeline statistics at a specific point in time
///
/// This struct contains immutable statistics collected during pipeline execution.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PipelineStats {
    /// Total number of nodes processed
    pub nodes_processed: u64,
    /// Total number of nodes that resulted in error
    pub nodes_failed: u64,
    /// Total number of nodes persisted to storage
    pub nodes_stored: u64,
    /// Total number of transformations applied
    pub transformations_applied: u64,
    /// Token usage per model
    pub token_usage: HashMap<String, ModelUsage>,
    /// When the pipeline started
    started_at: Option<Instant>,
    /// When the pipeline completed
    completed_at: Option<Instant>,
}

impl PipelineStats {
    /// Creates a new empty `PipelineStats`
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the duration of the pipeline execution
    ///
    /// If the pipeline has not started, returns `None`.
    /// If the pipeline has started but not completed, returns the elapsed time since start.
    #[must_use]
    pub fn duration(&self) -> Option<Duration> {
        match (self.started_at, self.completed_at) {
            (Some(start), Some(end)) => Some(end.duration_since(start)),
            (Some(start), None) => Some(start.elapsed()),
            _ => None,
        }
    }

    /// Calculates nodes processed per second
    ///
    /// Returns `None` if the pipeline hasn't started or if no nodes have been processed.
    #[must_use]
    pub fn nodes_per_second(&self) -> Option<f64> {
        let duration = self.duration()?;
        if duration.as_secs_f64() == 0.0 || self.nodes_processed == 0 {
            return None;
        }
        Some(self.nodes_processed as f64 / duration.as_secs_f64())
    }

    /// Returns the total number of tokens used across all models
    #[must_use]
    pub fn total_tokens(&self) -> u64 {
        self.token_usage.values().map(|u| u.total_tokens).sum()
    }

    /// Returns the total number of LLM requests made
    #[must_use]
    pub fn total_requests(&self) -> u64 {
        self.token_usage.values().map(|u| u.request_count).sum()
    }

    /// Returns the total prompt tokens across all models
    #[must_use]
    pub fn total_prompt_tokens(&self) -> u64 {
        self.token_usage.values().map(|u| u.prompt_tokens).sum()
    }

    /// Returns the total completion tokens across all models
    #[must_use]
    pub fn total_completion_tokens(&self) -> u64 {
        self.token_usage.values().map(|u| u.completion_tokens).sum()
    }
}

/// Thread-safe statistics collector for pipeline execution
///
/// This collector uses atomic counters for lock-free updates and can be safely
/// shared across multiple threads during pipeline processing.
#[derive(Debug)]
pub struct StatsCollector {
    nodes_processed: AtomicU64,
    nodes_failed: AtomicU64,
    nodes_stored: AtomicU64,
    transformations_applied: AtomicU64,
    token_usage: std::sync::Mutex<HashMap<String, ModelUsage>>,
    started_at: std::sync::Mutex<Option<Instant>>,
    completed_at: std::sync::Mutex<Option<Instant>>,
}

impl Default for StatsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl StatsCollector {
    /// Creates a new `StatsCollector`
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes_processed: AtomicU64::new(0),
            nodes_failed: AtomicU64::new(0),
            nodes_stored: AtomicU64::new(0),
            transformations_applied: AtomicU64::new(0),
            token_usage: std::sync::Mutex::new(HashMap::new()),
            started_at: std::sync::Mutex::new(None),
            completed_at: std::sync::Mutex::new(None),
        }
    }

    /// Marks the pipeline as started
    pub fn start(&self) {
        let mut started = self.started_at.lock().unwrap();
        *started = Some(Instant::now());
    }

    /// Marks the pipeline as completed
    pub fn complete(&self) {
        let mut completed = self.completed_at.lock().unwrap();
        *completed = Some(Instant::now());
    }

    /// Increments the count of processed nodes
    pub fn increment_nodes_processed(&self, count: u64) {
        self.nodes_processed.fetch_add(count, Ordering::Relaxed);
    }

    /// Increments the count of failed nodes
    pub fn increment_nodes_failed(&self, count: u64) {
        self.nodes_failed.fetch_add(count, Ordering::Relaxed);
    }

    /// Increments the count of stored nodes
    pub fn increment_nodes_stored(&self, count: u64) {
        self.nodes_stored.fetch_add(count, Ordering::Relaxed);
    }

    /// Increments the count of applied transformations
    pub fn increment_transformations(&self, count: u64) {
        self.transformations_applied.fetch_add(count, Ordering::Relaxed);
    }

    /// Records token usage for a specific model
    ///
    /// This method is compatible with OpenTelemetry LLM specification.
    ///
    /// # Arguments
    ///
    /// * `model` - The name/identifier of the model
    /// * `prompt_tokens` - Number of tokens in the prompt
    /// * `completion_tokens` - Number of tokens in the completion
    pub fn record_token_usage(&self, model: impl AsRef<str>, prompt_tokens: u64, completion_tokens: u64) {
        let mut usage = self.token_usage.lock().unwrap();
        let model_usage = usage
            .entry(model.as_ref().to_string())
            .or_insert_with(ModelUsage::new);
        model_usage.record(prompt_tokens, completion_tokens);
    }

    /// Returns a snapshot of the current statistics
    #[must_use]
    pub fn get_stats(&self) -> PipelineStats {
        PipelineStats {
            nodes_processed: self.nodes_processed.load(Ordering::Relaxed),
            nodes_failed: self.nodes_failed.load(Ordering::Relaxed),
            nodes_stored: self.nodes_stored.load(Ordering::Relaxed),
            transformations_applied: self.transformations_applied.load(Ordering::Relaxed),
            token_usage: self.token_usage.lock().unwrap().clone(),
            started_at: *self.started_at.lock().unwrap(),
            completed_at: *self.completed_at.lock().unwrap(),
        }
    }
}

impl Clone for StatsCollector {
    fn clone(&self) -> Self {
        Self {
            nodes_processed: AtomicU64::new(self.nodes_processed.load(Ordering::Relaxed)),
            nodes_failed: AtomicU64::new(self.nodes_failed.load(Ordering::Relaxed)),
            nodes_stored: AtomicU64::new(self.nodes_stored.load(Ordering::Relaxed)),
            transformations_applied: AtomicU64::new(
                self.transformations_applied.load(Ordering::Relaxed),
            ),
            token_usage: std::sync::Mutex::new(self.token_usage.lock().unwrap().clone()),
            started_at: std::sync::Mutex::new(*self.started_at.lock().unwrap()),
            completed_at: std::sync::Mutex::new(*self.completed_at.lock().unwrap()),
        }
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
        collector.increment_transformations(15);
        
        collector.complete();
        
        let stats = collector.get_stats();
        
        assert_eq!(stats.nodes_processed, 10);
        assert_eq!(stats.nodes_failed, 2);
        assert_eq!(stats.nodes_stored, 8);
        assert_eq!(stats.transformations_applied, 15);
        assert!(stats.duration().is_some());
        assert!(stats.nodes_per_second().is_some());
    }

    #[test]
    fn test_model_usage() {
        let mut usage = ModelUsage::new();
        
        usage.record(100, 50);
        usage.record(200, 100);
        
        assert_eq!(usage.prompt_tokens, 300);
        assert_eq!(usage.completion_tokens, 150);
        assert_eq!(usage.total_tokens, 450);
        assert_eq!(usage.request_count, 2);
    }

    #[test]
    fn test_record_token_usage() {
        let collector = StatsCollector::new();
        
        collector.record_token_usage("gpt-4", 100, 50);
        collector.record_token_usage("gpt-4", 200, 100);
        collector.record_token_usage("gpt-3.5", 50, 25);
        
        let stats = collector.get_stats();
        
        assert_eq!(stats.token_usage.len(), 2);
        
        let gpt4_usage = stats.token_usage.get("gpt-4").unwrap();
        assert_eq!(gpt4_usage.prompt_tokens, 300);
        assert_eq!(gpt4_usage.completion_tokens, 150);
        assert_eq!(gpt4_usage.request_count, 2);
        
        assert_eq!(stats.total_tokens(), 525);
        assert_eq!(stats.total_requests(), 3);
    }

    #[test]
    fn test_empty_stats() {
        let stats = PipelineStats::new();
        
        assert_eq!(stats.nodes_processed, 0);
        assert_eq!(stats.nodes_failed, 0);
        assert_eq!(stats.total_tokens(), 0);
        assert!(stats.duration().is_none());
        assert!(stats.nodes_per_second().is_none());
    }

    #[test]
    fn test_stats_collector_clone() {
        let collector = StatsCollector::new();
        collector.increment_nodes_processed(5);
        collector.record_token_usage("model-1", 10, 5);
        
        let cloned = collector.clone();
        
        // Modify original
        collector.increment_nodes_processed(3);
        
        // Cloned should have original value
        let cloned_stats = cloned.get_stats();
        assert_eq!(cloned_stats.nodes_processed, 5);
        
        // Original should have updated value
        let original_stats = collector.get_stats();
        assert_eq!(original_stats.nodes_processed, 8);
    }

    #[test]
    fn test_pipeline_stats_duration_while_running() {
        let collector = StatsCollector::new();
        collector.start();
        
        let stats = collector.get_stats();
        
        // Should return Some while running
        assert!(stats.duration().is_some());
        assert_eq!(stats.completed_at, None);
    }
}
