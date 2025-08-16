//! Traits in Swiftide allow for easy extendability
//!
//! All steps defined in the indexing pipeline and the generic transformers can also take a
//! trait. To bring your own transformers, models and loaders, all you need to do is implement the
//! trait and it should work out of the box.
use crate::Embeddings;
use crate::node::{Chunk, Node};
use crate::{
    SparseEmbeddings, indexing_defaults::IndexingDefaults, indexing_stream::IndexingStream,
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
    type Input: Chunk;
    type Output: Chunk;

    async fn transform_node(&self, node: Node<Self::Input>) -> Result<Node<Self::Output>>;

    /// Overrides the default concurrency of the pipeline
    fn concurrency(&self) -> Option<usize> {
        None
    }

    fn name(&self) -> &'static str {
        let name = std::any::type_name::<Self>();
        name.split("::").last().unwrap_or(name)
    }
}

dyn_clone::clone_trait_object!(<I, O> Transformer<Input = I, Output = O>);

// #[cfg(feature = "test-utils")]
// mock! {
//     #[derive(Debug)]
//     pub Transformer {}
//
//     #[async_trait]
//     impl<T: Chunk> Transformer for Transformer {
//         type ChunkType: Chunk;
//
//         async fn transform_node(&self, node: Node<T>) -> Result<Node>;
//         fn concurrency(&self) -> Option<usize>;
//         fn name(&self) -> &'static str;
//     }
//
//     impl Clone for Transformer {
//         fn clone(&self) -> Self;
//     }
// }

#[async_trait]
impl<I: Chunk, O: Chunk> Transformer for Box<dyn Transformer<Input = I, Output = O>> {
    type Input = I;
    type Output = O;

    async fn transform_node(&self, node: Node<Self::Input>) -> Result<Node<Self::Output>> {
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
impl<I: Chunk, O: Chunk> Transformer for Arc<dyn Transformer<Input = I, Output = O>> {
    type Input = I;
    type Output = O;

    async fn transform_node(&self, node: Node<Self::Input>) -> Result<Node<Self::Output>> {
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
impl<I: Chunk, O: Chunk> Transformer for &dyn Transformer<Input = I, Output = O> {
    type Input = I;
    type Output = O;

    async fn transform_node(&self, node: Node<Self::Input>) -> Result<Node<Self::Output>> {
        (*self).transform_node(node).await
    }
    fn concurrency(&self) -> Option<usize> {
        (*self).concurrency()
    }
}

#[async_trait]
/// Use a closure as a transformer
// TODO: Find a way to make this work with full generics
impl<F> Transformer for F
where
    F: Fn(Node<String>) -> Result<Node<String>> + Send + Sync + Clone,
{
    type Input = String;
    type Output = String;

    async fn transform_node(&self, node: Node<Self::Input>) -> Result<Node<Self::Output>> {
        self(node)
    }
}

#[async_trait]
/// Transforms batched single nodes into streams of nodes
pub trait BatchableTransformer: Send + Sync + DynClone {
    type Input: Chunk;
    type Output: Chunk;

    /// Transforms a batch of nodes into a stream of nodes
    async fn batch_transform(&self, nodes: Vec<Node<Self::Input>>) -> IndexingStream<Self::Output>;

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

dyn_clone::clone_trait_object!(<I, O> BatchableTransformer<Input = I, Output = O>);

// #[cfg(feature = "test-utils")]
// mock! {
//     #[derive(Debug)]
//     pub BatchableTransformer {}
//
//     #[async_trait]
//     impl BatchableTransformer for BatchableTransformer {
//         async fn batch_transform(&self, nodes: Vec<Node<T>>) -> IndexingStream;
//         fn name(&self) -> &'static str;
//         fn batch_size(&self) -> Option<usize>;
//         fn concurrency(&self) -> Option<usize>;
//     }
//
//     impl Clone for BatchableTransformer {
//         fn clone(&self) -> Self;
//     }
// }
#[async_trait]
/// Use a closure as a batchable transformer
impl<F> BatchableTransformer for F
where
    F: Fn(Vec<Node<String>>) -> IndexingStream<String> + Send + Sync + Clone,
{
    type Input = String;
    type Output = String;

    async fn batch_transform(&self, nodes: Vec<Node<String>>) -> IndexingStream<String> {
        self(nodes)
    }
}

#[async_trait]
impl BatchableTransformer for Box<dyn BatchableTransformer> {
    async fn batch_transform(&self, nodes: Vec<Node<T>>) -> IndexingStream {
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
    async fn batch_transform(&self, nodes: Vec<Node<T>>) -> IndexingStream {
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
    async fn batch_transform(&self, nodes: Vec<Node<T>>) -> IndexingStream {
        (*self).batch_transform(nodes).await
    }
    fn concurrency(&self) -> Option<usize> {
        (*self).concurrency()
    }
}

/// Starting point of a stream
pub trait Loader: DynClone + Send + Sync {
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
        unimplemented!(
            "Please implement into_stream_boxed for your loader, it needs to be implemented on the concrete type"
        )
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
pub trait ChunkerTransformer: Send + Sync + DynClone {
    async fn transform_node(&self, node: Node<T>) -> IndexingStream;

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
    async fn transform_node(&self, node: Node<T>) -> IndexingStream;
        fn name(&self) -> &'static str;
        fn concurrency(&self) -> Option<usize>;
    }

    impl Clone for ChunkerTransformer {
        fn clone(&self) -> Self;
    }
}
#[async_trait]
impl ChunkerTransformer for Box<dyn ChunkerTransformer> {
    async fn transform_node(&self, node: Node<T>) -> IndexingStream {
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
    async fn transform_node(&self, node: Node<T>) -> IndexingStream {
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
    async fn transform_node(&self, node: Node<T>) -> IndexingStream {
        (*self).transform_node(node).await
    }
    fn concurrency(&self) -> Option<usize> {
        (*self).concurrency()
    }
}

#[async_trait]
impl<F> ChunkerTransformer for F
where
    F: Fn(Node<T>) -> IndexingStream + Send + Sync + Clone,
{
    async fn transform_node(&self, node: Node<T>) -> IndexingStream {
        self(node)
    }
}

// #[cfg_attr(feature = "test-utils", automock)]
#[async_trait]
/// Caches nodes, typically by their path and hash
/// Recommended to namespace on the storage
///
/// For now just bool return value for easy filter
pub trait NodeCache: Send + Sync + Debug + DynClone {
    type ChunkType: Chunk;

    async fn get(&self, node: &Node<T>) -> bool;
    async fn set(&self, node: &Node<T>);

    /// Optionally provide a method to clear the cache
    async fn clear(&self) -> Result<()> {
        unimplemented!("Clear not implemented")
    }

    fn name(&self) -> &'static str {
        let name = std::any::type_name::<Self>();
        name.split("::").last().unwrap_or(name)
    }
}

dyn_clone::clone_trait_object!(<T: Chunk> NodeCache<T>);

#[cfg(feature = "test-utils")]
mock! {
    #[derive(Debug)]
    pub NodeCache<T: Chunk> {}

    #[async_trait]
    impl<T: Chunk> NodeCache<T> for NodeCache<T> {
        async fn get(&self, node: &Node<T>) -> bool;
        async fn set(&self, node: &Node<T>);
        async fn clear(&self) -> Result<()>;
        fn name(&self) -> &'static str;

    }

    impl<T: Chunk> Clone for NodeCache<T> {
        fn clone(&self) -> Self;
    }
}

#[async_trait]
impl<T: Chunk> NodeCache<T> for Box<dyn NodeCache<T>> {
    async fn get(&self, node: &Node<T>) -> bool {
        self.as_ref().get(node).await
    }
    async fn set(&self, node: &Node<T>) {
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
impl<T: Chunk> NodeCache<T> for Arc<dyn NodeCache<T>> {
    async fn get(&self, node: &Node<T>) -> bool {
        self.as_ref().get(node).await
    }
    async fn set(&self, node: &Node<T>) {
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
impl<T: Chunk> NodeCache<T> for &dyn NodeCache<T> {
    async fn get(&self, node: &Node<T>) -> bool {
        (*self).get(node).await
    }
    async fn set(&self, node: &Node<T>) {
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
    async fn store(&self, node: Node<T>) -> Result<Node>;
    async fn batch_store(&self, nodes: Vec<Node<T>>) -> IndexingStream;
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
        async fn store(&self, node: Node<T>) -> Result<Node>;
        async fn batch_store(&self, nodes: Vec<Node<T>>) -> IndexingStream;
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
    async fn store(&self, node: Node<T>) -> Result<Node> {
        self.as_ref().store(node).await
    }
    async fn batch_store(&self, nodes: Vec<Node<T>>) -> IndexingStream {
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
    async fn store(&self, node: Node<T>) -> Result<Node> {
        self.as_ref().store(node).await
    }
    async fn batch_store(&self, nodes: Vec<Node<T>>) -> IndexingStream {
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
    async fn store(&self, node: Node<T>) -> Result<Node> {
        (*self).store(node).await
    }
    async fn batch_store(&self, nodes: Vec<Node<T>>) -> IndexingStream {
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

impl<F> WithIndexingDefaults for F where F: Fn(Node<T>) -> Result<Node> {}
impl<F> WithBatchIndexingDefaults for F where F: Fn(Vec<Node<T>>) -> IndexingStream {}

#[cfg(feature = "test-utils")]
impl WithIndexingDefaults for MockTransformer {}
//
#[cfg(feature = "test-utils")]
impl WithBatchIndexingDefaults for MockBatchableTransformer {}
