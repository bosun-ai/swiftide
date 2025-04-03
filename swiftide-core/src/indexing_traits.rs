//! Traits in Swiftide allow for easy extendability
//!
//! All steps defined in the indexing pipeline and the generic transformers can also take a
//! trait. To bring your own transformers, models and loaders, all you need to do is implement the
//! trait and it should work out of the box.
use crate::node::Node;
use crate::Embeddings;
use crate::{
    indexing_defaults::IndexingDefaults, indexing_stream::IndexingStream, SparseEmbeddings,
};
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;

use crate::chat_completion::errors::LanguageModelError;
use crate::prompt::Prompt;
use anyhow::Result;
use async_trait::async_trait;

pub use dyn_clone::DynClone;
/// All traits are easily mockable under tests
#[cfg(feature = "test-utils")]
#[doc(hidden)]
use mockall::{mock, predicate::str};

#[async_trait]
/// Transforms single nodes into single nodes
pub trait Transformer: Send + Sync + DynClone {
    async fn transform_node(&self, node: Node) -> Result<Node>;

    /// Overrides the default concurrency of the pipeline
    fn concurrency(&self) -> Option<usize> {
        None
    }

    fn name(&self) -> &'static str {
        let name = std::any::type_name::<Self>();
        name.split("::").last().unwrap_or(name)
    }
}

dyn_clone::clone_trait_object!(Transformer);

#[cfg(feature = "test-utils")]
mock! {
    #[derive(Debug)]
    pub Transformer {}

    #[async_trait]
    impl Transformer for Transformer {
        async fn transform_node(&self, node: Node) -> Result<Node>;
        fn concurrency(&self) -> Option<usize>;
        fn name(&self) -> &'static str;
    }

    impl Clone for Transformer {
        fn clone(&self) -> Self;
    }
}

#[async_trait]
impl Transformer for Box<dyn Transformer> {
    async fn transform_node(&self, node: Node) -> Result<Node> {
        self.as_ref().transform_node(node).await
    }
    fn concurrency(&self) -> Option<usize> {
        self.as_ref().concurrency()
    }
    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

#[async_trait]
impl Transformer for Arc<dyn Transformer> {
    async fn transform_node(&self, node: Node) -> Result<Node> {
        self.as_ref().transform_node(node).await
    }
    fn concurrency(&self) -> Option<usize> {
        self.as_ref().concurrency()
    }
    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

#[async_trait]
impl Transformer for &dyn Transformer {
    async fn transform_node(&self, node: Node) -> Result<Node> {
        (*self).transform_node(node).await
    }
    fn concurrency(&self) -> Option<usize> {
        (*self).concurrency()
    }
}

#[async_trait]
/// Use a closure as a transformer
impl<F> Transformer for F
where
    F: Fn(Node) -> Result<Node> + Send + Sync + Clone,
{
    async fn transform_node(&self, node: Node) -> Result<Node> {
        self(node)
    }
}

#[async_trait]
/// Transforms batched single nodes into streams of nodes
pub trait BatchableTransformer: Send + Sync + DynClone {
    /// Transforms a batch of nodes into a stream of nodes
    async fn batch_transform(&self, nodes: Vec<Node>) -> IndexingStream;

    /// Overrides the default concurrency of the pipeline
    fn concurrency(&self) -> Option<usize> {
        None
    }

    fn name(&self) -> &'static str {
        let name = std::any::type_name::<Self>();
        name.split("::").last().unwrap_or(name)
    }

    /// Overrides the default batch size of the pipeline
    fn batch_size(&self) -> Option<usize> {
        None
    }
}

dyn_clone::clone_trait_object!(BatchableTransformer);

#[cfg(feature = "test-utils")]
mock! {
    #[derive(Debug)]
    pub BatchableTransformer {}

    #[async_trait]
    impl BatchableTransformer for BatchableTransformer {
        async fn batch_transform(&self, nodes: Vec<Node>) -> IndexingStream;
        fn name(&self) -> &'static str;
        fn batch_size(&self) -> Option<usize>;
        fn concurrency(&self) -> Option<usize>;
    }

    impl Clone for BatchableTransformer {
        fn clone(&self) -> Self;
    }
}
#[async_trait]
/// Use a closure as a batchable transformer
impl<F> BatchableTransformer for F
where
    F: Fn(Vec<Node>) -> IndexingStream + Send + Sync + Clone,
{
    async fn batch_transform(&self, nodes: Vec<Node>) -> IndexingStream {
        self(nodes)
    }
}

#[async_trait]
impl BatchableTransformer for Box<dyn BatchableTransformer> {
    async fn batch_transform(&self, nodes: Vec<Node>) -> IndexingStream {
        self.as_ref().batch_transform(nodes).await
    }
    fn concurrency(&self) -> Option<usize> {
        self.as_ref().concurrency()
    }
    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

#[async_trait]
impl BatchableTransformer for Arc<dyn BatchableTransformer> {
    async fn batch_transform(&self, nodes: Vec<Node>) -> IndexingStream {
        self.as_ref().batch_transform(nodes).await
    }
    fn concurrency(&self) -> Option<usize> {
        self.as_ref().concurrency()
    }
    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

#[async_trait]
impl BatchableTransformer for &dyn BatchableTransformer {
    async fn batch_transform(&self, nodes: Vec<Node>) -> IndexingStream {
        (*self).batch_transform(nodes).await
    }
    fn concurrency(&self) -> Option<usize> {
        (*self).concurrency()
    }
}

/// Starting point of a stream
pub trait Loader: DynClone {
    fn into_stream(self) -> IndexingStream;

    /// Intended for use with Box<dyn Loader>
    ///
    /// Only needed if you use trait objects (Box<dyn Loader>)
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn into_stream_boxed(self: Box<Self>) -> IndexingStream {
    ///    self.into_stream()
    ///  }
    /// ```
    fn into_stream_boxed(self: Box<Self>) -> IndexingStream {
        unimplemented!("Please implement into_stream_boxed for your loader, it needs to be implemented on the concrete type")
    }

    fn name(&self) -> &'static str {
        let name = std::any::type_name::<Self>();
        name.split("::").last().unwrap_or(name)
    }
}

dyn_clone::clone_trait_object!(Loader);

#[cfg(feature = "test-utils")]
mock! {
    #[derive(Debug)]
    pub Loader {}

    #[async_trait]
    impl Loader for Loader {
        fn into_stream(self) -> IndexingStream;
        fn into_stream_boxed(self: Box<Self>) -> IndexingStream;
        fn name(&self) -> &'static str;
    }

    impl Clone for Loader {
        fn clone(&self) -> Self;
    }
}

impl Loader for Box<dyn Loader> {
    fn into_stream(self) -> IndexingStream {
        Loader::into_stream_boxed(self)
    }

    fn into_stream_boxed(self: Box<Self>) -> IndexingStream {
        Loader::into_stream(*self)
    }
    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

impl Loader for &dyn Loader {
    fn into_stream(self) -> IndexingStream {
        Loader::into_stream_boxed(Box::new(self))
    }

    fn into_stream_boxed(self: Box<Self>) -> IndexingStream {
        Loader::into_stream(*self)
    }
}

#[async_trait]
/// Turns one node into many nodes
pub trait ChunkerTransformer: Send + Sync + Debug + DynClone {
    async fn transform_node(&self, node: Node) -> IndexingStream;

    /// Overrides the default concurrency of the pipeline
    fn concurrency(&self) -> Option<usize> {
        None
    }

    fn name(&self) -> &'static str {
        let name = std::any::type_name::<Self>();
        name.split("::").last().unwrap_or(name)
    }
}

dyn_clone::clone_trait_object!(ChunkerTransformer);

#[cfg(feature = "test-utils")]
mock! {
    #[derive(Debug)]
    pub ChunkerTransformer {}

    #[async_trait]
    impl ChunkerTransformer for ChunkerTransformer {
    async fn transform_node(&self, node: Node) -> IndexingStream;
        fn name(&self) -> &'static str;
        fn concurrency(&self) -> Option<usize>;
    }

    impl Clone for ChunkerTransformer {
        fn clone(&self) -> Self;
    }
}
#[async_trait]
impl ChunkerTransformer for Box<dyn ChunkerTransformer> {
    async fn transform_node(&self, node: Node) -> IndexingStream {
        self.as_ref().transform_node(node).await
    }
    fn concurrency(&self) -> Option<usize> {
        self.as_ref().concurrency()
    }
    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

#[async_trait]
impl ChunkerTransformer for Arc<dyn ChunkerTransformer> {
    async fn transform_node(&self, node: Node) -> IndexingStream {
        self.as_ref().transform_node(node).await
    }
    fn concurrency(&self) -> Option<usize> {
        self.as_ref().concurrency()
    }
    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

#[async_trait]
impl ChunkerTransformer for &dyn ChunkerTransformer {
    async fn transform_node(&self, node: Node) -> IndexingStream {
        (*self).transform_node(node).await
    }
    fn concurrency(&self) -> Option<usize> {
        (*self).concurrency()
    }
}

// #[cfg_attr(feature = "test-utils", automock)]
#[async_trait]
/// Caches nodes, typically by their path and hash
/// Recommended to namespace on the storage
///
/// For now just bool return value for easy filter
pub trait NodeCache: Send + Sync + Debug + DynClone {
    async fn get(&self, node: &Node) -> bool;
    async fn set(&self, node: &Node);

    /// Optionally provide a method to clear the cache
    async fn clear(&self) -> Result<()> {
        unimplemented!("Clear not implemented")
    }

    fn name(&self) -> &'static str {
        let name = std::any::type_name::<Self>();
        name.split("::").last().unwrap_or(name)
    }
}

dyn_clone::clone_trait_object!(NodeCache);

#[cfg(feature = "test-utils")]
mock! {
    #[derive(Debug)]
    pub NodeCache {}

    #[async_trait]
    impl NodeCache for NodeCache {
        async fn get(&self, node: &Node) -> bool;
        async fn set(&self, node: &Node);
        async fn clear(&self) -> Result<()>;
        fn name(&self) -> &'static str;

    }

    impl Clone for NodeCache {
        fn clone(&self) -> Self;
    }
}

#[async_trait]
impl NodeCache for Box<dyn NodeCache> {
    async fn get(&self, node: &Node) -> bool {
        self.as_ref().get(node).await
    }
    async fn set(&self, node: &Node) {
        self.as_ref().set(node).await;
    }
    async fn clear(&self) -> Result<()> {
        self.as_ref().clear().await
    }
    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

#[async_trait]
impl NodeCache for Arc<dyn NodeCache> {
    async fn get(&self, node: &Node) -> bool {
        self.as_ref().get(node).await
    }
    async fn set(&self, node: &Node) {
        self.as_ref().set(node).await;
    }
    async fn clear(&self) -> Result<()> {
        self.as_ref().clear().await
    }
    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

#[async_trait]
impl NodeCache for &dyn NodeCache {
    async fn get(&self, node: &Node) -> bool {
        (*self).get(node).await
    }
    async fn set(&self, node: &Node) {
        (*self).set(node).await;
    }
    async fn clear(&self) -> Result<()> {
        (*self).clear().await
    }
}

#[async_trait]
/// Embeds a list of strings and returns its embeddings.
/// Assumes the strings will be moved.
pub trait EmbeddingModel: Send + Sync + Debug + DynClone {
    async fn embed(&self, input: Vec<String>) -> Result<Embeddings, LanguageModelError>;

    fn name(&self) -> &'static str {
        let name = std::any::type_name::<Self>();
        name.split("::").last().unwrap_or(name)
    }
}

dyn_clone::clone_trait_object!(EmbeddingModel);

#[cfg(feature = "test-utils")]
mock! {
    #[derive(Debug)]
    pub EmbeddingModel {}

    #[async_trait]
    impl EmbeddingModel for EmbeddingModel {
        async fn embed(&self, input: Vec<String>) -> Result<Embeddings, LanguageModelError>;
        fn name(&self) -> &'static str;
    }

    impl Clone for EmbeddingModel {
        fn clone(&self) -> Self;
    }
}

#[async_trait]
impl EmbeddingModel for Box<dyn EmbeddingModel> {
    async fn embed(&self, input: Vec<String>) -> Result<Embeddings, LanguageModelError> {
        self.as_ref().embed(input).await
    }

    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

#[async_trait]
impl EmbeddingModel for Arc<dyn EmbeddingModel> {
    async fn embed(&self, input: Vec<String>) -> Result<Embeddings, LanguageModelError> {
        self.as_ref().embed(input).await
    }

    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

#[async_trait]
impl EmbeddingModel for &dyn EmbeddingModel {
    async fn embed(&self, input: Vec<String>) -> Result<Embeddings, LanguageModelError> {
        (*self).embed(input).await
    }
}

#[async_trait]
/// Embeds a list of strings and returns its embeddings.
/// Assumes the strings will be moved.
pub trait SparseEmbeddingModel: Send + Sync + Debug + DynClone {
    async fn sparse_embed(
        &self,
        input: Vec<String>,
    ) -> Result<SparseEmbeddings, LanguageModelError>;

    fn name(&self) -> &'static str {
        let name = std::any::type_name::<Self>();
        name.split("::").last().unwrap_or(name)
    }
}

dyn_clone::clone_trait_object!(SparseEmbeddingModel);

#[cfg(feature = "test-utils")]
mock! {
    #[derive(Debug)]
    pub SparseEmbeddingModel {}

    #[async_trait]
    impl SparseEmbeddingModel for SparseEmbeddingModel {
        async fn sparse_embed(&self, input: Vec<String>) -> Result<SparseEmbeddings, LanguageModelError>;
        fn name(&self) -> &'static str;
    }

    impl Clone for SparseEmbeddingModel {
        fn clone(&self) -> Self;
    }
}

#[async_trait]
impl SparseEmbeddingModel for Box<dyn SparseEmbeddingModel> {
    async fn sparse_embed(
        &self,
        input: Vec<String>,
    ) -> Result<SparseEmbeddings, LanguageModelError> {
        self.as_ref().sparse_embed(input).await
    }

    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

#[async_trait]
impl SparseEmbeddingModel for Arc<dyn SparseEmbeddingModel> {
    async fn sparse_embed(
        &self,
        input: Vec<String>,
    ) -> Result<SparseEmbeddings, LanguageModelError> {
        self.as_ref().sparse_embed(input).await
    }

    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

#[async_trait]
impl SparseEmbeddingModel for &dyn SparseEmbeddingModel {
    async fn sparse_embed(
        &self,
        input: Vec<String>,
    ) -> Result<SparseEmbeddings, LanguageModelError> {
        (*self).sparse_embed(input).await
    }
}

/// Backoff configuration for api calls.
/// Each time an api call fails backoff will wait an increasing period of time for each subsequent
/// retry attempt. see <https://docs.rs/backoff/latest/backoff/> for more details.
#[derive(Debug, Clone, Copy)]
pub struct BackoffConfiguration {
    /// Initial interval in seconds between retries
    pub initial_interval_sec: u64,
    /// The factor by which the interval is multiplied on each retry attempt
    pub multiplier: f64,
    /// Introduces randomness to avoid retry storms
    pub randomization_factor: f64,
    /// Total time all attempts are allowed in seconds. Once a retry must wait longer than this,
    /// the request is considered to have failed.
    pub max_elapsed_time_sec: u64,
}

impl Default for BackoffConfiguration {
    fn default() -> Self {
        Self {
            initial_interval_sec: 1,
            multiplier: 2.0,
            randomization_factor: 0.5,
            max_elapsed_time_sec: 60,
        }
    }
}

#[async_trait]
/// Given a string prompt, queries an LLM
pub trait SimplePrompt: Debug + Send + Sync + DynClone {
    // Takes a simple prompt, prompts the llm and returns the response
    async fn prompt(&self, prompt: Prompt) -> Result<String, LanguageModelError>;

    fn name(&self) -> &'static str {
        let name = std::any::type_name::<Self>();
        name.split("::").last().unwrap_or(name)
    }
}

dyn_clone::clone_trait_object!(SimplePrompt);

#[derive(Debug, Clone)]
pub struct ResilientLanguageModel<P: Clone> {
    pub(crate) inner: P,
    config: BackoffConfiguration,
}

impl<P: Clone> ResilientLanguageModel<P> {
    pub fn new(client: P, config: BackoffConfiguration) -> Self {
        Self {
            inner: client,
            config,
        }
    }

    pub(crate) fn strategy(&self) -> backoff::ExponentialBackoff {
        backoff::ExponentialBackoffBuilder::default()
            .with_initial_interval(Duration::from_secs(self.config.initial_interval_sec))
            .with_multiplier(self.config.multiplier)
            .with_max_elapsed_time(Some(Duration::from_secs(self.config.max_elapsed_time_sec)))
            .with_randomization_factor(self.config.randomization_factor)
            .build()
    }
}

#[async_trait]
impl<P: SimplePrompt + Clone> SimplePrompt for ResilientLanguageModel<P> {
    async fn prompt(&self, prompt: Prompt) -> Result<String, LanguageModelError> {
        let strategy = self.strategy();

        let op = || {
            let prompt = prompt.clone();
            async {
                self.inner.prompt(prompt).await.map_err(|e| match e {
                    LanguageModelError::ContextLengthExceeded(e) => {
                        backoff::Error::Permanent(LanguageModelError::ContextLengthExceeded(e))
                    }
                    LanguageModelError::PermanentError(e) => {
                        backoff::Error::Permanent(LanguageModelError::PermanentError(e))
                    }
                    LanguageModelError::TransientError(e) => {
                        backoff::Error::transient(LanguageModelError::TransientError(e))
                    }
                })
            }
        };

        backoff::future::retry(strategy, op).await
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
}

#[async_trait]
impl<P: EmbeddingModel + Clone> EmbeddingModel for ResilientLanguageModel<P> {
    async fn embed(&self, input: Vec<String>) -> Result<Embeddings, LanguageModelError> {
        self.inner.embed(input).await
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
}

#[cfg(feature = "test-utils")]
mock! {
    #[derive(Debug)]
    pub SimplePrompt {}

    #[async_trait]
    impl SimplePrompt for SimplePrompt {
        async fn prompt(&self, prompt: Prompt) -> Result<String, LanguageModelError>;
        fn name(&self) -> &'static str;
    }

    impl Clone for SimplePrompt {
        fn clone(&self) -> Self;
    }
}

#[async_trait]
impl SimplePrompt for Box<dyn SimplePrompt> {
    async fn prompt(&self, prompt: Prompt) -> Result<String, LanguageModelError> {
        self.as_ref().prompt(prompt).await
    }

    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

#[async_trait]
impl SimplePrompt for Arc<dyn SimplePrompt> {
    async fn prompt(&self, prompt: Prompt) -> Result<String, LanguageModelError> {
        self.as_ref().prompt(prompt).await
    }

    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

#[async_trait]
impl SimplePrompt for &dyn SimplePrompt {
    async fn prompt(&self, prompt: Prompt) -> Result<String, LanguageModelError> {
        (*self).prompt(prompt).await
    }
}

#[async_trait]
/// Persists nodes
pub trait Persist: Debug + Send + Sync + DynClone {
    async fn setup(&self) -> Result<()>;
    async fn store(&self, node: Node) -> Result<Node>;
    async fn batch_store(&self, nodes: Vec<Node>) -> IndexingStream;
    fn batch_size(&self) -> Option<usize> {
        None
    }

    fn name(&self) -> &'static str {
        let name = std::any::type_name::<Self>();
        name.split("::").last().unwrap_or(name)
    }
}

dyn_clone::clone_trait_object!(Persist);

#[cfg(feature = "test-utils")]
mock! {
    #[derive(Debug)]
    pub Persist {}

    #[async_trait]
    impl Persist for Persist {
        async fn setup(&self) -> Result<()>;
        async fn store(&self, node: Node) -> Result<Node>;
        async fn batch_store(&self, nodes: Vec<Node>) -> IndexingStream;
        fn batch_size(&self) -> Option<usize>;

        fn name(&self) -> &'static str;
    }

    impl Clone for Persist {
        fn clone(&self) -> Self;
    }
}

#[async_trait]
impl Persist for Box<dyn Persist> {
    async fn setup(&self) -> Result<()> {
        self.as_ref().setup().await
    }
    async fn store(&self, node: Node) -> Result<Node> {
        self.as_ref().store(node).await
    }
    async fn batch_store(&self, nodes: Vec<Node>) -> IndexingStream {
        self.as_ref().batch_store(nodes).await
    }
    fn batch_size(&self) -> Option<usize> {
        self.as_ref().batch_size()
    }
    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

#[async_trait]
impl Persist for Arc<dyn Persist> {
    async fn setup(&self) -> Result<()> {
        self.as_ref().setup().await
    }
    async fn store(&self, node: Node) -> Result<Node> {
        self.as_ref().store(node).await
    }
    async fn batch_store(&self, nodes: Vec<Node>) -> IndexingStream {
        self.as_ref().batch_store(nodes).await
    }
    fn batch_size(&self) -> Option<usize> {
        self.as_ref().batch_size()
    }
    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

#[async_trait]
impl Persist for &dyn Persist {
    async fn setup(&self) -> Result<()> {
        (*self).setup().await
    }
    async fn store(&self, node: Node) -> Result<Node> {
        (*self).store(node).await
    }
    async fn batch_store(&self, nodes: Vec<Node>) -> IndexingStream {
        (*self).batch_store(nodes).await
    }
    fn batch_size(&self) -> Option<usize> {
        (*self).batch_size()
    }
}

/// Allows for passing defaults from the pipeline to the transformer
/// Required for batch transformers as at least a marker, implementation is not required
pub trait WithIndexingDefaults {
    fn with_indexing_defaults(&mut self, _indexing_defaults: IndexingDefaults) {}
}

/// Allows for passing defaults from the pipeline to the batch transformer
/// Required for batch transformers as at least a marker, implementation is not required
pub trait WithBatchIndexingDefaults {
    fn with_indexing_defaults(&mut self, _indexing_defaults: IndexingDefaults) {}
}

impl WithIndexingDefaults for dyn Transformer {}
impl WithIndexingDefaults for Box<dyn Transformer> {
    fn with_indexing_defaults(&mut self, indexing_defaults: IndexingDefaults) {
        self.as_mut().with_indexing_defaults(indexing_defaults);
    }
}
impl WithBatchIndexingDefaults for dyn BatchableTransformer {}
impl WithBatchIndexingDefaults for Box<dyn BatchableTransformer> {
    fn with_indexing_defaults(&mut self, indexing_defaults: IndexingDefaults) {
        self.as_mut().with_indexing_defaults(indexing_defaults);
    }
}

impl<F> WithIndexingDefaults for F where F: Fn(Node) -> Result<Node> {}
impl<F> WithBatchIndexingDefaults for F where F: Fn(Vec<Node>) -> IndexingStream {}

#[cfg(feature = "test-utils")]
impl WithIndexingDefaults for MockTransformer {}
//
#[cfg(feature = "test-utils")]
impl WithBatchIndexingDefaults for MockBatchableTransformer {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    struct MockSimplePrompt {
        call_count: Arc<AtomicUsize>,
        should_fail_count: usize,
        error_type: MockErrorType,
    }

    #[derive(Debug, Clone, Copy)]
    enum MockErrorType {
        Transient,
        Permanent,
        ContextLengthExceeded,
    }

    #[async_trait]
    impl SimplePrompt for MockSimplePrompt {
        async fn prompt(&self, _prompt: Prompt) -> Result<String, LanguageModelError> {
            let count = self.call_count.fetch_add(1, Ordering::SeqCst);

            if count < self.should_fail_count {
                match self.error_type {
                    MockErrorType::Transient => Err(LanguageModelError::TransientError(Box::new(
                        std::io::Error::new(std::io::ErrorKind::ConnectionReset, "Transient error"),
                    ))),
                    MockErrorType::Permanent => Err(LanguageModelError::PermanentError(Box::new(
                        std::io::Error::new(std::io::ErrorKind::InvalidData, "Permanent error"),
                    ))),
                    MockErrorType::ContextLengthExceeded => Err(
                        LanguageModelError::ContextLengthExceeded(Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "Context length exceeded",
                        ))),
                    ),
                }
            } else {
                Ok("Success response".to_string())
            }
        }

        fn name(&self) -> &'static str {
            "MockSimplePrompt"
        }
    }

    #[tokio::test]
    async fn test_resilient_language_model_retries_transient_errors() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let mock_prompt = MockSimplePrompt {
            call_count: call_count.clone(),
            should_fail_count: 2, // Fail twice, succeed on third attempt
            error_type: MockErrorType::Transient,
        };

        let config = BackoffConfiguration {
            initial_interval_sec: 1,
            max_elapsed_time_sec: 10,
            multiplier: 1.5,
            randomization_factor: 0.5,
        };

        let resilient_model = ResilientLanguageModel::new(mock_prompt, config);

        let result = resilient_model.prompt(Prompt::from("Test prompt")).await;

        assert!(result.is_ok());
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
        assert_eq!(result.unwrap(), "Success response");
    }

    #[tokio::test]
    async fn test_resilient_language_model_does_not_retry_permanent_errors() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let mock_prompt = MockSimplePrompt {
            call_count: call_count.clone(),
            should_fail_count: 1,
            error_type: MockErrorType::Permanent,
        };

        let config = BackoffConfiguration {
            initial_interval_sec: 1,
            max_elapsed_time_sec: 10,
            multiplier: 1.5,
            randomization_factor: 0.5,
        };

        let resilient_model = ResilientLanguageModel::new(mock_prompt, config);

        let result = resilient_model.prompt(Prompt::from("Test prompt")).await;

        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        match result {
            Err(LanguageModelError::PermanentError(_)) => {} // Expected
            _ => panic!("Expected PermanentError"),
        }
    }

    #[tokio::test]
    async fn test_resilient_language_model_does_not_retry_context_length_errors() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let mock_prompt = MockSimplePrompt {
            call_count: call_count.clone(),
            should_fail_count: 1,
            error_type: MockErrorType::ContextLengthExceeded,
        };

        let config = BackoffConfiguration {
            initial_interval_sec: 1,
            max_elapsed_time_sec: 10,
            multiplier: 1.5,
            randomization_factor: 0.5,
        };

        let resilient_model = ResilientLanguageModel::new(mock_prompt, config);

        let result = resilient_model.prompt(Prompt::from("Test prompt")).await;

        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        match result {
            Err(LanguageModelError::ContextLengthExceeded(_)) => {} // Expected
            _ => panic!("Expected ContextLengthExceeded"),
        }
    }
}
