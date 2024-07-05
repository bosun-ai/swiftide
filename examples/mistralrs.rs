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
    integrations::{fastembed::FastEmbed, huggingface_mistralrs::MistralPrompt},
    loaders::FileLoader,
    persist::MemoryStorage,
    transformers::{Embed, MetadataSummary},
};

fn setup() -> anyhow::Result<Arc<MistralRs>> {
    let loader = NormalLoaderBuilder::new(
        NormalSpecificConfig {
            use_flash_attn: false,
            repeat_last_n: 64,
        },
        None,
        None,
        Some("google/gemma-2-9b-it".to_string()),
    )
    .build(NormalLoaderType::Gemma2);
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

    tracing::info!("Setting up mistralrs");
    let mistralrs = setup()?;

    tracing::info!("Getting mistralrs sender");
    let mistral_sender = mistralrs.get_sender()?;

    let storage = MemoryStorage::default();

    ingestion::IngestionPipeline::from_loader(FileLoader::new("README.md"))
        .then(MetadataSummary::new(
            MistralPrompt::from_mistral_sender(mistral_sender).build()?,
        ))
        .log_all()
        .then_in_batch(10, Embed::new(FastEmbed::try_default()?))
        .then_store_with(storage.clone())
        .log_nodes()
        .run()
        .await?;

    println!("Summaries:");
    println!(
        "{}",
        storage
            .get_all_values()
            .await
            .iter()
            .filter_map(|n| n.metadata.get("Summary"))
            .cloned()
            .collect::<Vec<_>>()
            .join("\n---\n")
    );
    Ok(())
}
