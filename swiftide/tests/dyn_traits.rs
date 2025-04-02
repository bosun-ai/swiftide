//! Tests for dyn trait objects
use swiftide::{indexing::transformers::ChunkCode, integrations};
use swiftide_core::{
    BatchableTransformer, ChunkerTransformer, EmbeddingModel, Loader, NodeCache, Persist,
    SimplePrompt, Transformer,
};
use swiftide_indexing::{loaders, transformers};
use swiftide_integrations::fastembed::FastEmbed;

#[test_log::test(tokio::test)]
async fn test_name_on_dyn() {
    let fastembed: Box<dyn EmbeddingModel> = Box::new(FastEmbed::try_default().unwrap());

    assert_eq!(fastembed.name(), "FastEmbed");

    let chunk_code: Box<dyn ChunkerTransformer> =
        Box::new(ChunkCode::try_for_language("rust").unwrap());
    assert_eq!(chunk_code.name(), "ChunkCode");

    let transformer: Box<dyn Transformer> = Box::new(transformers::MetadataQAText::default());
    assert_eq!(transformer.name(), "MetadataQAText");

    let redis: Box<dyn NodeCache> = Box::new(
        integrations::redis::Redis::try_from_url("redis://localhost:6379", "prefix").unwrap(),
    );
    assert_eq!(redis.name(), "Redis");

    let embed: Box<dyn BatchableTransformer> =
        Box::new(transformers::Embed::new(fastembed).with_batch_size(10));
    assert_eq!(embed.name(), "Embed");

    let qdrant: Box<dyn Persist> = Box::new(
        integrations::qdrant::Qdrant::try_from_url("http://localhost:6333")
            .unwrap()
            .vector_size(1536)
            .build()
            .unwrap(),
    );
    assert_eq!(qdrant.name(), "Qdrant");

    let openai_client: Box<dyn SimplePrompt> = Box::new(
        integrations::openai::OpenAI::builder()
            .default_embed_model("text-embedding-3-small")
            .default_prompt_model("gpt-3.5-turbo")
            .build()
            .unwrap(),
    );
    assert_eq!(openai_client.name(), "GenericOpenAI");

    let loader: Box<dyn Loader> = Box::new(loaders::FileLoader::new(".").with_extensions(&["rs"]));
    assert_eq!(loader.name(), "FileLoader");
}
