/// This example demonstrates how to use the LanceDB integration with Swiftide
use swiftide::{
    indexing::{
        self,
        loaders::FileLoader,
        transformers::{
            metadata_qa_text::NAME as METADATA_QA_TEXT_NAME, ChunkMarkdown, Embed, MetadataQAText,
        },
        EmbeddedField,
    },
    integrations::{self, lancedb::LanceDB},
    query::{self, answers, query_transformers, response_transformers},
};
use temp_dir::TempDir;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let openai_client = integrations::openai::OpenAI::builder()
        .default_embed_model("text-embedding-3-small")
        .default_prompt_model("gpt-4o-mini")
        .build()?;

    let tempdir = TempDir::new().unwrap();

    // Configure lancedb with a default vector size, a single embedding
    // and in addition to embedding the text metadata, also store it in a field
    let lancedb = LanceDB::builder()
        .uri(tempdir.child("lancedb").to_str().unwrap())
        .vector_size(1536)
        .with_vector(EmbeddedField::Combined)
        .with_metadata(METADATA_QA_TEXT_NAME)
        .table_name("swiftide_test")
        .build()
        .unwrap();

    indexing::Pipeline::from_loader(FileLoader::new("README.md"))
        .then_chunk(ChunkMarkdown::from_chunk_range(10..2048))
        .then(MetadataQAText::new(openai_client.clone()))
        .then_in_batch(Embed::new(openai_client.clone()).with_batch_size(10))
        .then_store_with(lancedb.clone())
        .run()
        .await?;

    // By default the search strategy is SimilaritySingleEmbedding
    // which takes the latest query, embeds it, and does a similarity search
    //
    // LanceDB will return an error if multiple embeddings are set
    //
    // The pipeline generates subquestions to increase semantic coverage, embeds these in a single
    // embedding, retrieves the default top_k documents, summarizes them and uses that as context
    // for the final answer.
    let pipeline = query::Pipeline::default()
        .then_transform_query(query_transformers::GenerateSubquestions::from_client(
            openai_client.clone(),
        ))
        .then_transform_query(query_transformers::Embed::from_client(
            openai_client.clone(),
        ))
        .then_retrieve(lancedb.clone())
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
