use anyhow::Result;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use swiftide::indexing::transformers::ChunkCode;
use swiftide::{
    indexing::{loaders::FileLoader, persist::MemoryStorage, Pipeline},
    traits::NodeCache,
};
use temp_dir::TempDir;
use testcontainers::Container;
use testcontainers::{
    core::{IntoContainerPort, WaitFor},
    runners::SyncRunner,
    GenericImage,
};

async fn run_pipeline(node_cache: Box<dyn NodeCache>) -> Result<()> {
    Pipeline::from_loader(FileLoader::new(".").with_extensions(&["rs"]))
        .filter_cached(node_cache)
        .then_chunk(ChunkCode::try_for_language_and_chunk_size("rust", 10..256)?)
        .then_store_with(MemoryStorage::default())
        .run()
        .await
}

fn criterion_benchmark(c: &mut Criterion) {
    let redis_container = start_redis();

    let redis_url = format!(
        "redis://{host}:{port}",
        host = redis_container.get_host().unwrap(),
        port = redis_container.get_host_port_ipv4(6379).unwrap()
    );

    let redis: Box<dyn NodeCache> = Box::new(
        swiftide::integrations::redis::Redis::try_from_url(redis_url, "criterion").unwrap(),
    );

    let tempdir = TempDir::new().unwrap();
    let redb: Box<dyn NodeCache> = Box::new(
        swiftide::integrations::redb::Redb::builder()
            .database_path(tempdir.child("criterion"))
            .build()
            .unwrap(),
    );

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    for node_cache in [(redis, "redis"), (redb, "redb")] {
        c.bench_with_input(
            BenchmarkId::new("node_cache", node_cache.1),
            &node_cache,
            |b, s| {
                let cache_clone = s.0.clone();
                runtime.spawn_blocking(move || async move { cache_clone.clear().await.unwrap() });

                b.to_async(&runtime).iter(|| run_pipeline(s.0.clone()))
            },
        );
    }
}

fn start_redis() -> Container<GenericImage> {
    GenericImage::new("redis", "7.2.4")
        .with_exposed_port(6379.tcp())
        .with_wait_for(WaitFor::message_on_stdout("Ready to accept connections"))
        .start()
        .expect("Redis started")
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
