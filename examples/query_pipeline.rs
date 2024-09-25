use swiftide::{
    indexing::{
        self,
        loaders::FileLoader,
        transformers::{ChunkMarkdown, Embed, MetadataQAText},
    },
    integrations::{self, qdrant::Qdrant},
    query::{self, answers, query_transformers, response_transformers},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let openai_client = integrations::openai::OpenAI::builder()
        .default_embed_model("text-embedding-3-large")
        .default_prompt_model("gpt-4o")
        .build()?;

    let qdrant = Qdrant::builder()
        .batch_size(50)
        .vector_size(3072)
        .collection_name("swiftide-examples")
        .build()?;

    indexing::Pipeline::from_loader(FileLoader::new("README.md"))
        .then_chunk(ChunkMarkdown::from_chunk_range(10..2048))
        .then(MetadataQAText::new(openai_client.clone()))
        .then_in_batch(Embed::new(openai_client.clone()).with_batch_size(10))
        .then_store_with(qdrant.clone())
        .run()
        .await?;

    // By default the search strategy is SimilaritySingleEmbedding
    // which takes the latest query, embeds it, and does a similarity search
    let pipeline = query::Pipeline::default()
        .then_transform_query(query_transformers::GenerateSubquestions::from_client(
            openai_client.clone(),
        ))
        .then_transform_query(query_transformers::Embed::from_client(
            openai_client.clone(),
        ))
        .then_retrieve(qdrant.clone())
        .then_transform_response(response_transformers::Summary::from_client(
            openai_client.clone(),
        ))
        .then_answer(answers::Simple::from_client(openai_client.clone()));

    let result = pipeline
        .query("What is swiftide? Please provide an elaborate explanation")
        .await?;

    println!("{:?}", result.answer());
    Ok(())
}
