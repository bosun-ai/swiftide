use anyhow::Result;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use futures_util::stream::{StreamExt, TryStreamExt};
use swiftide::traits::Loader;

async fn run_fileloader(num_files: usize) -> Result<usize> {
    let mut total_nodes = 0;
    let mut stream = swiftide::indexing::loaders::FileLoader::new("./benchmarks/fileloader.rs")
        .with_extensions(&["rs"])
        .into_stream()
        .take(num_files);

    while stream.try_next().await?.is_some() {
        total_nodes += 1;
    }
    assert!(total_nodes == num_files);
    Ok(total_nodes)
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("load_1", |b| b.iter(|| run_fileloader(black_box(1))));
    c.bench_function("load_10", |b| b.iter(|| run_fileloader(black_box(10))));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
