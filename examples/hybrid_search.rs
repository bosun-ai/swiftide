//! # [Swiftide] Hybrid search with qudrant
//!
//! This example demonstrates how to do hybrid search with Qdrant with Sparse vectors.
//!
//! [Swiftide]: https://github.com/bosun-ai/swiftide
//! [examples]: https://github.com/bosun-ai/swiftide/blob/master/examples

use swiftide::{
    indexing::{
        self,
        loaders::FileLoader,
        transformers::{self, ChunkCode, MetadataQACode},
        EmbeddedField,
    },
    integrations::{fastembed::FastEmbed, openai, qdrant::Qdrant},
    query::{self, answers, query_transformers, search_strategies::HybridSearch},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    // Ensure all batching is consistent
    let batch_size = 64;

    let fastembed_sparse = FastEmbed::try_default_sparse().unwrap().to_owned();
    let fastembed = FastEmbed::try_default().unwrap().to_owned();

    // Set up openai with the mini model, which is great for indexing
    let openai = openai::OpenAI::builder()
        .default_prompt_model("gpt-4o-mini")
        .build()
        .unwrap();

    // Set up qdrant and use the combined fields (metadata + chunks) for both sparse and dense
    // vectors
    let qdrant = Qdrant::builder()
        .batch_size(batch_size)
        .vector_size(384)
        .with_vector(EmbeddedField::Combined)
        .with_sparse_vector(EmbeddedField::Combined)
        .collection_name("swiftide-hybrid-example")
        .build()?;

    indexing::Pipeline::from_loader(FileLoader::new("swiftide-core/").with_extensions(&["rs"]))
        // Chunk fairly large as the context window is big
        .then_chunk(ChunkCode::try_for_language_and_chunk_size(
            "rust",
            10..2048,
        )?)
        // Generate metadata on the code chunks to increase our chances of finding the right code
        .then(MetadataQACode::from_client(openai.clone()).build().unwrap())
        .then_in_batch(
            transformers::SparseEmbed::new(fastembed_sparse.clone()).with_batch_size(batch_size),
        )
        .then_in_batch(transformers::Embed::new(fastembed.clone()).with_batch_size(batch_size))
        .then_store_with(qdrant.clone())
        .run()
        .await?;

    // Use sophisticated model for our query
    let openai = openai::OpenAI::builder()
        .default_prompt_model("gpt-4o")
        .build()
        .unwrap();

    let query_pipeline = query::Pipeline::from_search_strategy(
        // Return a large amount of documents because we have a large context window
        // By default it uses the Combined fields, no need to configure
        HybridSearch::default()
            .with_top_n(20)
            .with_top_k(20)
            .to_owned(),
    )
    // Generate subquestions on the initial query to increase our query coverage
    .then_transform_query(query_transformers::GenerateSubquestions::from_client(
        openai.clone(),
    ))
    // Generate the same embeddings we used for indexing
    .then_transform_query(query_transformers::Embed::from_client(fastembed.clone()))
    .then_transform_query(query_transformers::SparseEmbed::from_client(
        fastembed_sparse.clone(),
    ))
    .then_retrieve(qdrant.clone())
    // Answer with Simple, which either takes the documents as is (in this case), or any
    // transformations applied after querying
    .then_answer(answers::Simple::from_client(openai.clone()));

    let answer = query_pipeline
        .query("What are the different pipelines in Swiftide and how do they work? Provide an elaborate answer with examples.")
        .await
        .unwrap();

    println!("{}", answer.answer());

    // ## Different Pipelines in Swiftide and How They Work
    //
    // Swiftide offers multiple pipelines, notably the indexing pipeline and the query pipeline. The
    // functionality of these pipelines is enhanced using traits and components like transformers,
    // stream handlers, and more. Below we elaborate on the key components and how they become part
    // of the larger pipeline system:
    //
    // ### Indexing Pipeline
    //
    // 1. **Transformers**:
    //     - **Transformer Trait**: Transforms single nodes into single nodes. Mainly used for
    //       transforming data in a singular manner.
    //     - **BatchableTransformer Trait**: Transforms a batch of nodes into a stream of nodes,
    //       useful for bulk processing.
    //
    //     ```rust
    //     #[async_trait]
    //     pub trait Transformer: Send + Sync {
    //         async fn transform_node(&self, node: Node) -> Result<Node>;
    //         fn concurrency(&self) -> Option<usize> { None }
    //     }
    //
    //     #[async_trait]
    //     impl<F> Transformer for F where F: Fn(Node) -> Result<Node> + Send + Sync {
    //         async fn transform_node(&self, node: Node) -> Result<Node> {
    //             self(node)
    //         }
    //     }
    //
    //     #[async_trait]
    //     pub trait BatchableTransformer: Send + Sync {
    //         async fn batch_transform(&self, nodes: Vec<Node>) -> IndexingStream;
    //         fn concurrency(&self) -> Option<usize> { None }
    //     }
    //
    //     #[async_trait]
    //     impl<F> BatchableTransformer for F where F: Fn(Vec<Node>) -> IndexingStream + Send + Sync
    // {         async fn batch_transform(&self, nodes: Vec<Node>) -> IndexingStream {
    //             self(nodes)
    //         }
    //     }
    //     ```
    //
    // 2. **Loaders**:
    //     - Defines methods for converting a loader into an `IndexingStream`.
    //
    //     ```rust
    //     pub trait Loader {
    //         fn into_stream(self) -> IndexingStream;
    //     }
    //     ```
    //
    // 3. **Chunker Transformers**:
    //     - Splits one node into multiple nodes. It's useful for breaking down large nodes into
    //       smaller, manageable chunks.
    //
    //     ```rust
    //     #[async_trait]
    //     pub trait ChunkerTransformer: Send + Sync + Debug {
    //         async fn transform_node(&self, node: Node) -> IndexingStream;
    //         fn concurrency(&self) -> Option<usize> { None }
    //     }
    //     ```
    //
    // 4. **IndexingStream**:
    //     - An asynchronous stream of nodes, used internally by the indexing pipeline to handle
    //       streams of `Node` items.
    //
    //     ```rust
    //     pub struct IndexingStream {
    //         #[pin]
    //         pub(crate) inner: Pin<Box<dyn Stream<Item = Result<Node>> + Send>>,
    //     }
    //     ```
    //
    // ### Query Pipeline
    //
    // 1. **QueryStream**:
    //     - Handles query streams, ensuring data flows correctly through various query states.
    //
    //     ```rust
    //     pub struct QueryStream<'stream, Q: 'stream> {
    //         #[pin]
    //         pub(crate) inner: Pin<Box<dyn Stream<Item = Result<Query<Q>>> + Send + 'stream>>,
    //         #[pin]
    //         pub sender: Option<Sender<Result<Query<Q>>>>,
    //     }
    //     ```
    //
    // 2. **Query Handling**:
    //     - Various state transitions and handling for queries in the pipeline.
    //
    //     ```rust
    //     pub struct Query<State> {
    //         original: String,
    //         current: String,
    //         state: State,
    //         transformation_history: Vec<TransformationEvent>,
    //         pub embedding: Option<Embedding>,
    //         pub sparse_embedding: Option<SparseEmbedding>,
    //     }
    //     ```
    //
    // ### Extending the Pipeline with Traits
    //
    // Swiftide allows developers to extend the pipeline by implementing custom transformers,
    // loaders, and other components by implementing the respective traits. This design ensures
    // flexibility and modularity, allowing seamless integration of custom functionality.
    //
    // For example, to create a custom transformer:
    // ```rust
    // use crate::node::Node;
    // use anyhow::Result;
    //
    // struct MyCustomTransformer;
    //
    // #[async_trait]
    // impl Transformer for MyCustomTransformer {
    //     async fn transform_node(&self, node: Node) -> Result<Node> {
    //         // Custom transformation logic here...
    //         Ok(node)
    //     }
    // }
    // ```
    //
    // ### Usage of Prompts in Transformers
    //
    // Swiftide utilizes the [`Template`] for templating prompts, making it easy to define and
    // manage prompts within transformers.
    //
    // ```rust
    // let template = PromptTemplate::try_compiled_from_str("hello {{world}}").await.unwrap();
    // let prompt = template.to_prompt().with_context_value("world", "swiftide");
    // assert_eq!(prompt.render().await.unwrap(), "hello swiftide");
    // ```
    //
    // ### Conclusion
    //
    // The Indexing and Query Pipelines in Swiftide are made extensible and modular via traits such
    // as `Transformer`, `BatchableTransformer`, `Loader`, and more. Custom implementations can
    // seamlessly integrate into the pipeline, providing flexibility in how data is processed,
    // transformed, and indexed. The use of prompts further enhances the capability to manage
    // dynamic and templated data within these pipelines.

    Ok(())
}
