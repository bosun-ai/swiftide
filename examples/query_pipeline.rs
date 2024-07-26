use swiftide::{
    indexing::{
        self,
        loaders::FileLoader,
        transformers::{ChunkCode, ChunkMarkdown, Embed, MetadataQACode, MetadataQAText},
    },
    integrations::{self, qdrant::Qdrant, redis::Redis},
    query::{self, query_transformers, retrievers, search_strategies},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let openai_client = integrations::openai::OpenAI::builder()
        .default_embed_model("text-embedding-3-small")
        .default_prompt_model("gpt-3.5-turbo")
        .build()?;

    let qdrant = Qdrant::builder()
        .batch_size(50)
        .vector_size(1536)
        .collection_name("swiftide-examples")
        .build()?;

    indexing::Pipeline::from_loader(FileLoader::new("README.md"))
        .then_chunk(ChunkMarkdown::from_chunk_range(10..2048))
        .then(MetadataQAText::new(openai_client.clone()))
        .then_in_batch(10, Embed::new(openai_client.clone()))
        .then_store_with(qdrant.clone())
        .run()
        .await?;

    let pipeline = query::Pipeline::default()
        .with_search_strategy(search_strategies::SimilaritySingleEmbedding::default())
        .then_transform_query(query_transformers::GenerateSubquestions::from_client(
            openai_client.clone(),
        ))
        .then_transform_query(query_transformers::Embed::from_client(
            openai_client.clone(),
        ))
        .then_retrieve(qdrant.clone())
        .then_transform_response(response_transformers::Summary::default())
        .then_answer(answers::Simple::default());

    let result = pipeline.query("What is swiftide?").await?;
    println!("{:?}", result);
    Ok(())
}
