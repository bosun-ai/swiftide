use swiftide::{
    indexing,
    integrations::{self, qdrant::Qdrant, redis::Redis},
    loaders::FileLoader,
    transformers::{ChunkCode, ChunkMarkdown, Embed, MetadataQACode, MetadataQAText},
};
use swiftide_query::{query, query_transformers, search_strategy::SimilaritySingleEmbedding};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let openai_client = integrations::openai::OpenAI::builder()
        .default_embed_model("text-embedding-3-small")
        .default_prompt_model("gpt-3.5-turbo")
        .build()?;

    indexing::Pipeline::from_loader(FileLoader::new("README.md"))
        .then_chunk(ChunkMarkdown::from_chunk_range(10..2048))
        .then(MetadataQAText::new(openai_client.clone()))
        .then_in_batch(10, Embed::new(openai_client.clone()))
        .then_store_with(
            Qdrant::builder()
                .batch_size(50)
                .vector_size(1536)
                .collection_name("swiftide-examples")
                .build()?,
        )
        .run()
        .await?;

    let pipeline = query::Pipeline::default()
        .with_search_strategy(SimilaritySingleEmbedding::default())
        .then_transform_query(query_transformers::GenerateSubquestions::from_client(
            openai_client.clone(),
        ))
        .then_transform_query(query_transformers::Embed::from_client(
            openai_client.clone(),
        ))
        .then_retrieve(retrievers::Qdrant::default())
        .then_transform_response(response_transformers::Summary::default())
        .then_answer(answers::Simple::default());

    let result = pipeline.query("What is swiftide?").await?;
    println!("{:?}", result);
    Ok(())
}
