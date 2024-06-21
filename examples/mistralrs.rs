//! # [Swiftide] Ingesting using MistralRs
//!
//! This example shows how to ingest the Swiftide codebase using a MistralRs model.
//!
//! For this example to work, make sure you are authenticated with the huggingface cli and have a
//! beefy machine.
//!
//!
//! [Swiftide]: https://github.com/bosun-ai/swiftide
//! [examples]: https://github.com/bosun-ai/swiftide/blob/master/examples

use std::sync::Arc;

use mistralrs::{
    Device, DeviceMapMetadata, MistralRs, MistralRsBuilder, ModelDType, NormalLoaderBuilder,
    NormalLoaderType, NormalSpecificConfig, SchedulerMethod, TokenSource,
};
use swiftide::{
    ingestion,
    integrations::{self, huggingface_mistralrs::MistralPrompt, qdrant::Qdrant},
    loaders::FileLoader,
    transformers::{Embed, MetadataQACode},
};

fn setup() -> anyhow::Result<Arc<MistralRs>> {
    // Select a Mistral model
    let loader = NormalLoaderBuilder::new(
        NormalSpecificConfig {
            use_flash_attn: false,
            repeat_last_n: 64,
        },
        None,
        None,
        Some("mistralai/Mistral-7B-Instruct-v0.1".to_string()),
    )
    .build(NormalLoaderType::Mistral);
    // Load, into a Pipeline
    let pipeline = loader.load_model_from_hf(
        None,
        TokenSource::CacheToken,
        &ModelDType::Auto,
        &Device::cuda_if_available(0)?,
        false,
        DeviceMapMetadata::dummy(),
        None,
    )?;
    // Create the MistralRs, which is a runner
    Ok(MistralRsBuilder::new(pipeline, SchedulerMethod::Fixed(5.try_into().unwrap())).build())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let openai_client = integrations::openai::OpenAI::builder()
        .default_embed_model("text-embedding-3-small")
        .default_prompt_model("gpt-3.5-turbo")
        .build()?;

    let qdrant_url = std::env::var("QDRANT_URL")
        .as_deref()
        .unwrap_or("http://localhost:6334")
        .to_owned();

    let mistralrs = setup()?;

    ingestion::IngestionPipeline::from_loader(FileLoader::new(".").with_extensions(&["rs"]))
        .then(MetadataQACode::new(
            MistralPrompt::from_mistral_sender(mistralrs.get_sender()?).build()?,
        ))
        .then_in_batch(10, Embed::new(openai_client.clone()))
        .then_store_with(
            Qdrant::try_from_url(qdrant_url)?
                .batch_size(50)
                .vector_size(1536)
                .collection_name("swiftide-examples-mistralrs".to_string())
                .build()?,
        )
        .run()
        .await?;
    Ok(())
}
