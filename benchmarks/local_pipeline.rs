use anyhow::Result;
use criterion::{criterion_group, criterion_main, Criterion};
use swiftide::{
    ingestion::IngestionPipeline,
    integrations::{fastembed::FastEmbed, qdrant::Qdrant},
    loaders::FileLoader,
    transformers::Embed,
};

async fn run_pipeline() -> Result<()> {
    let qdrant_url = "http://localhost:6334";
    IngestionPipeline::from_loader(FileLoader::new(".").with_extensions(&["rs"]))
        .then_in_batch(10, Embed::new(FastEmbed::builder().batch_size(10).build()?))
        .then_store_with(
            Qdrant::try_from_url(qdrant_url)?
                .batch_size(50)
                .vector_size(384)
                .collection_name("swiftide-examples-fastembed".to_string())
                .build()?,
        )
        .run()
        .await
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("run_local_pipeline", |b| b.iter(run_pipeline));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
