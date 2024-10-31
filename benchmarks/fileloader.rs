use anyhow::Result;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use futures_util::stream::{StreamExt, TryStreamExt};
use swiftide::traits::{AsyncLoader, Loader};

async fn run_fileloader(num_files: usize) -> Result<usize> {
    let mut total_nodes = 0;
    let mut stream = Loader::into_stream(
        swiftide::indexing::loaders::FileLoader::new("../").with_extensions(&["rs"]),
    )
    .take(num_files);

    while stream.try_next().await?.is_some() {
        total_nodes += 1;
    }
    assert!(
        total_nodes == num_files,
        "Expected {} nodes, got {}",
        num_files,
        total_nodes
    );
    Ok(total_nodes)
}

async fn run_fileloader_async(num_files: usize) -> Result<usize> {
    let mut total_nodes = 0;
    let mut stream = AsyncLoader::into_stream(
        swiftide::indexing::loaders::FileLoader::new("../").with_extensions(&["rs"]),
    )
    .await
    .take(num_files);

    while stream.try_next().await?.is_some() {
        total_nodes += 1;
    }
    assert!(
        total_nodes == num_files,
        "Expected {} nodes, got {}",
        num_files,
        total_nodes
    );
    Ok(total_nodes)
}

fn criterion_benchmark(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    [1, 10, 100].iter().for_each(|size| {
        c.bench_with_input(
            BenchmarkId::new(format!("sync_loader_{size}"), size),
            &size,
            |b, &s| {
                b.to_async(&runtime)
                    .iter(|| run_fileloader(black_box(*s as usize)));
            },
        );
        c.bench_with_input(
            BenchmarkId::new(format!("async_sync_loader_{size}"), size),
            &size,
            |b, &s| {
                b.to_async(&runtime)
                    .iter(|| run_fileloader_async(black_box(*s as usize)));
            },
        );
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
