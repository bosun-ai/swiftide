use anyhow::Result;
use criterion::{criterion_group, criterion_main, Criterion};
use swiftide::{
    indexing::loaders::FileLoader,
    indexing::persist::MemoryStorage,
    indexing::transformers::{ChunkMarkdown, Embed},
    indexing::Pipeline,
    integrations::fastembed::FastEmbed,
};

async fn run_pipeline() -> Result<()> {
    Pipeline::from_loader(FileLoader::new("README.md").with_extensions(&["md"]))
        .then_chunk(ChunkMarkdown::from_chunk_range(20..256))
        .then_in_batch(Embed::new(FastEmbed::builder().batch_size(10).build()?))
        .then_store_with(MemoryStorage::default())
        .run()
        .await
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("run_local_pipeline", |b| b.iter(run_pipeline));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
