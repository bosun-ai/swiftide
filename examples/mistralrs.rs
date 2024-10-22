use swiftide::{
    indexing::{
        self,
        loaders::FileLoader,
        persist::MemoryStorage,
        transformers::{ChunkMarkdown, Embed, MetadataQAText},
    },
    integrations,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let embedding_model = integrations::fastembed::FastEmbed::try_default()?;

    let prompt_model = integrations::mistralrs::Mistralrs::builder()
        .model_name("microsoft/Phi-3.5-mini-instruct")
        .build()?;

    let memory_storage = MemoryStorage::default();

    indexing::Pipeline::from_loader(FileLoader::new("README.md"))
        .then_chunk(ChunkMarkdown::from_chunk_range(10..2048))
        .then(MetadataQAText::new(prompt_model))
        .then_in_batch(Embed::new(embedding_model))
        .then_store_with(memory_storage.clone())
        .run()
        .await?;

    println!("{:#?}", memory_storage.get_all_values().await);

    Ok(())
}
