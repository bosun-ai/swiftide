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
