use swiftide::{
    indexing::{
        self,
        loaders::FileLoader,
        transformers::{metadata_qa_text, ChunkMarkdown, Embed, MetadataQAText},
        EmbeddedField,
    },
    integrations::{dashscope::DashscopeBuilder, lancedb::LanceDB},
    query::{
        self,
        answers::{self},
        query_transformers::{self},
        response_transformers,
    },
};
use temp_dir::TempDir;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let client = DashscopeBuilder::default()
        .default_embed_model("text-embedding-v2")
        .default_prompt_model("qwen-long")
        .default_dimensions(1536)
        .build()?;
    let tempdir = TempDir::new().unwrap();
    let lancedb = LanceDB::builder()
        .uri(tempdir.child("lancedb").to_str().unwrap())
        .vector_size(1536)
        .with_vector(EmbeddedField::Combined)
        .with_metadata(metadata_qa_text::NAME)
        .table_name("swiftide_test")
        .build()
        .unwrap();

    indexing::Pipeline::from_loader(FileLoader::new(".").with_extensions(&["md"]))
        .with_default_llm_client(client.clone())
        .then_chunk(ChunkMarkdown::from_chunk_range(10..2048))
        .then(MetadataQAText::new(client.clone()))
        .then_in_batch(Embed::new(client.clone()).with_batch_size(10))
        .then_store_with(lancedb.clone())
        .run()
        .await?;

    let pipeline = query::Pipeline::default()
        .then_transform_query(query_transformers::GenerateSubquestions::from_client(
            client.clone(),
        ))
        .then_transform_query(query_transformers::Embed::from_client(client.clone()))
        .then_retrieve(lancedb.clone())
        .then_transform_response(response_transformers::Summary::from_client(client.clone()))
        .then_answer(answers::Simple::from_client(client.clone()));

    let result = pipeline
        .query("What is swiftide? Please provide an elaborate explanation")
        .await?;

    println!("====");
    println!("{:?}", result.answer());
    Ok(())
}
