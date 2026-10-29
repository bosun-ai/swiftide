#![cfg(feature = "lazy-executor")]
use std::{path::Path, sync::Arc};
use swiftide_agents::tools::local_executor::LocalExecutor;
use swiftide_agents_contrib::executors::{LazyExecutor, ToolExecutorExt};
#[tokio::test]
async fn source_binary_download_initializes_and_verifies_the_complete_file() {
    let directory = temp_dir::TempDir::new().unwrap();
    let bytes: Vec<u8> = (0..=255).cycle().take(4096).collect();
    std::fs::write(directory.path().join("remote.bin"), &bytes).unwrap();
    let workdir = directory.path().to_path_buf();
    let lazy = LazyExecutor::new(move || {
        let workdir = workdir.clone();
        async move {
            Ok(
                Arc::new(LocalExecutor::builder().workdir(workdir).build().unwrap())
                    as Arc<dyn ToolExecutorExt>,
            )
        }
    });
    lazy.cleanup().await.unwrap();
    assert!(!lazy.is_started());
    let local = directory.path().join("local.bin");
    assert_eq!(
        lazy.download(Path::new("remote.bin"), &local, 4096)
            .await
            .unwrap(),
        4096
    );
    assert!(lazy.is_started());
    assert_eq!(std::fs::read(local).unwrap(), bytes);
    assert!(
        lazy.download(
            Path::new("remote.bin"),
            &directory.path().join("too-large.bin"),
            4095
        )
        .await
        .is_err()
    );
    assert!(!directory.path().join("too-large.bin").exists());
}

#[tokio::test]
async fn refused_download_preserves_existing_destination() {
    let dir = temp_dir::TempDir::new().unwrap();
    let local = dir.path().join("local.bin");
    std::fs::write(&local, b"keep me").unwrap();
    std::fs::write(dir.path().join("large.bin"), b"too large").unwrap();
    let executor = LocalExecutor::builder()
        .workdir(dir.path())
        .build()
        .unwrap();
    for remote in ["missing.bin", "large.bin"] {
        assert!(
            executor
                .download(Path::new(remote), &local, 1)
                .await
                .is_err()
        );
        assert_eq!(std::fs::read(&local).unwrap(), b"keep me");
    }
}
