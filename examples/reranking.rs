/// Demonstrates reranking retrieved documents with fastembed
///
/// When reranking, many more documents are retrieved than used for the initial query. Maybe
/// even from multiple sources.
///
/// Reranking compares the relevancy of the documents with the initial query, then filters out
/// the `top_k` documents.
///
/// By default the model uses 'bge-reranker-base'.
use swiftide::{
    indexing::{
        self,
        loaders::FileLoader,
        transformers::{ChunkMarkdown, Embed},
    },
    integrations::{self, fastembed, qdrant::Qdrant},
    query::{self, answers, query_transformers},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let openai_client = integrations::openai::OpenAI::builder()
        .default_prompt_model("gpt-4o")
        .build()?;

    let fastembed = fastembed::FastEmbed::builder().batch_size(10).build()?;
    let reranker = fastembed::Rerank::builder().top_k(5).build()?;

    let qdrant = Qdrant::builder()
        .batch_size(50)
        .vector_size(384)
        .collection_name("swiftide-reranking")
        .build()?;

    indexing::Pipeline::from_loader(FileLoader::new("README.md"))
        .then_chunk(ChunkMarkdown::from_chunk_range(10..2048))
        .then_in_batch(Embed::new(fastembed.clone()))
        .then_store_with(qdrant.clone())
        .run()
        .await?;

    // By default the search strategy is SimilaritySingleEmbedding
    // which takes the latest query, embeds it, and does a similarity search
    let pipeline = query::Pipeline::default()
        .then_transform_query(query_transformers::GenerateSubquestions::from_client(
            openai_client.clone(),
        ))
        .then_transform_query(query_transformers::Embed::from_client(fastembed.clone()))
        .then_retrieve(qdrant.clone())
        .then_transform_response(reranker)
        .then_answer(answers::Simple::from_client(openai_client.clone()));

    let result = pipeline
        .query("What is swiftide? Please provide an elaborate explanation")
        .await?;

    println!("{:?}", result.answer());
    Ok(())
}
