#![cfg(feature = "file-tools")]

use std::{path::Path, sync::Arc};
use swiftide_agents::{DefaultContext, tools::local_executor::LocalExecutor};
use swiftide_agents_contrib::tools::ApplyPatch;
use swiftide_core::{
    ToolExecutor,
    chat_completion::{Tool, ToolCall, ToolOutput},
};

struct Workspace {
    directory: temp_dir::TempDir,
    context: DefaultContext,
}

#[tokio::test]
async fn moving_a_file_onto_itself_is_rejected_without_deleting_it() {
    for (source, destination) in [
        ("one.txt", "one.txt"),
        ("one.txt", "./one.txt"),
        ("./one.txt", "one.txt"),
    ] {
        let workspace = Workspace::new(&[("one.txt", "old\n")]);
        let report = workspace.patch(&format!("*** Begin Patch\n*** Update File: {source}\n*** Move to: {destination}\n@@\n-old\n+new\n*** End Patch")).await;
        assert!(report.contains("destination must differ"), "{report}");
        assert_eq!(workspace.read("one.txt"), "old\n");
    }
}

#[derive(Clone)]
struct RefuseRemoval(Arc<dyn ToolExecutor>);

#[async_trait::async_trait]
impl ToolExecutor for RefuseRemoval {
    async fn exec_cmd(
        &self,
        command: &swiftide_core::Command,
    ) -> Result<swiftide_core::CommandOutput, swiftide_core::CommandError> {
        if matches!(command, swiftide_core::Command::Shell { command, .. } if command.starts_with("rm -- "))
        {
            return Err(swiftide_core::CommandError::ExecutorError(
                std::io::Error::other("permission denied").into(),
            ));
        }
        self.0.exec_cmd(command).await
    }

    async fn stream_files(
        &self,
        path: &Path,
        extensions: Option<Vec<String>>,
    ) -> anyhow::Result<swiftide_core::indexing::IndexingStream<String>> {
        self.0.stream_files(path, extensions).await
    }
}

#[tokio::test]
async fn failed_move_removal_reports_both_files_actual_state() {
    let workspace = Workspace::new(&[("old.txt", "old\n")]);
    let executor: Arc<dyn ToolExecutor> = Arc::new(RefuseRemoval(Arc::new(
        LocalExecutor::builder()
            .workdir(workspace.directory.path())
            .build()
            .unwrap(),
    )));
    let context = DefaultContext::from_executor(executor);
    let patch = "*** Begin Patch\n*** Update File: old.txt\n*** Move to: new.txt\n@@\n-old\n+new\n*** End Patch";
    let output = ApplyPatch
        .invoke(
            &context,
            &ToolCall::builder()
                .id("move")
                .name("apply_patch")
                .args(serde_json::json!({"patch":patch}).to_string())
                .build()
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        output
            .content()
            .unwrap()
            .contains("destination new.txt was written, but source old.txt was not removed")
    );
    assert_eq!(workspace.read("old.txt"), "old\n");
    assert_eq!(workspace.read("new.txt"), "new\n");
}

impl Workspace {
    fn new(files: &[(&str, &str)]) -> Self {
        let directory = temp_dir::TempDir::new().unwrap();
        for (path, content) in files {
            let path = directory.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }
        let executor: Arc<dyn ToolExecutor> = Arc::new(
            LocalExecutor::builder()
                .workdir(directory.path())
                .build()
                .unwrap(),
        );
        Self {
            directory,
            context: DefaultContext::from_executor(executor),
        }
    }

    async fn patch(&self, patch: &str) -> String {
        self.invoke(patch)
            .await
            .unwrap()
            .as_text()
            .unwrap()
            .to_owned()
    }

    async fn invoke(
        &self,
        patch: &str,
    ) -> Result<ToolOutput, swiftide_core::chat_completion::errors::ToolError> {
        ApplyPatch
            .invoke(
                &self.context,
                &ToolCall::builder()
                    .name("apply_patch")
                    .id("patch")
                    .args(serde_json::json!({"patch":patch}).to_string())
                    .build()
                    .unwrap(),
            )
            .await
    }

    fn read(&self, path: impl AsRef<Path>) -> String {
        std::fs::read_to_string(self.directory.path().join(path)).unwrap()
    }
}

#[tokio::test]
async fn adds_moves_updates_and_deletes_files_with_quoted_paths() {
    let workspace = Workspace::new(&[("old.txt", "hello\nworld\n"), ("delete.txt", "gone")]);
    let report = workspace.patch("*** Begin Patch\n*** Add File: -folder/new 'file.txt\n+new\n*** Update File: old.txt\n*** Move to: nested/moved.txt\n@@\n hello\n-world\n+updated\n*** Delete File: delete.txt\n*** End Patch").await;
    assert!(
        report.starts_with("Patch applied successfully (3/3"),
        "{report}"
    );
    assert_eq!(workspace.read("nested/moved.txt"), "hello\nupdated\n");
    assert_eq!(workspace.read("-folder/new 'file.txt"), "new\n");
    assert!(!workspace.directory.path().join("old.txt").exists());
    assert!(!workspace.directory.path().join("delete.txt").exists());
    ApplyPatch.tool_spec().strict_parameters_schema().unwrap();
}

#[tokio::test]
async fn named_anchors_disambiguate_repeated_text_and_retries_detect_applied_work() {
    let workspace = Workspace::new(&[(
        "code.rs",
        "fn first() {\n    old();\n}\nfn second() {\n    old();\n}\n",
    )]);
    let patch = "*** Begin Patch\n*** Update File: code.rs\n@@ fn second()\n-    old();\n+    new();\n*** End Patch";
    assert!(
        workspace
            .patch(patch)
            .await
            .starts_with("Patch applied successfully")
    );
    assert_eq!(
        workspace.read("code.rs"),
        "fn first() {\n    old();\n}\nfn second() {\n    new();\n}\n"
    );
    let retry = workspace.patch(patch).await;
    assert!(retry.contains("Already present:"), "{retry}");
}

#[tokio::test]
async fn ambiguous_updates_leave_the_file_unchanged() {
    let original = "section\nvalue\nsection\nvalue\n";
    let workspace = Workspace::new(&[("repeated.txt", original)]);
    let report = workspace.patch("*** Begin Patch\n*** Update File: repeated.txt\n@@\n section\n-value\n+updated\n*** End Patch").await;
    assert!(report.contains("ambiguous"));
    assert_eq!(workspace.read("repeated.txt"), original);
}

#[tokio::test]
async fn reports_partial_success_and_an_interrupted_patch_can_be_retried() {
    let workspace = Workspace::new(&[("one.txt", "old\n"), ("two.txt", "stale\n")]);
    let patch = "*** Begin Patch\n*** Update File: one.txt\n@@\n-old\n+new\n*** Update File: two.txt\n@@\n-expected\n+updated\n*** End Patch";
    let report = workspace.patch(patch).await;
    assert!(report.starts_with("Patch partially applied (1/2"));
    assert_eq!(workspace.read("one.txt"), "new\n");
    assert_eq!(workspace.read("two.txt"), "stale\n");
    std::fs::write(workspace.directory.path().join("two.txt"), "expected\n").unwrap();
    let report = workspace.patch(patch).await;
    assert!(
        report.starts_with("Patch applied successfully (2/2"),
        "{report}"
    );
    assert!(report.contains("Already present:"));
    assert_eq!(workspace.read("two.txt"), "updated\n");
}

#[tokio::test]
async fn rejects_parent_absolute_and_empty_paths_without_writing() {
    let workspace = Workspace::new(&[]);
    for path in ["../outside.txt", "/absolute.txt"] {
        let report = workspace
            .patch(&format!(
                "*** Begin Patch\n*** Add File: {path}\n+text\n*** End Patch"
            ))
            .await;
        assert!(report.starts_with("Patch not applied"), "{report}");
    }
    assert!(
        workspace
            .invoke("*** Begin Patch\n*** Add File: \n+text\n*** End Patch")
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read_dir(workspace.directory.path())
            .unwrap()
            .count(),
        0
    );
}

#[tokio::test]
async fn malformed_patches_are_rejected_before_any_operation_executes() {
    let workspace = Workspace::new(&[]);
    for patch in [
        "",
        "*** Begin Patch\n*** End Patch",
        "*** Begin Patch\n*** Add File: would-create.txt\n+text\n*** Surprise\n*** End Patch",
    ] {
        assert!(workspace.invoke(patch).await.is_err());
    }
    assert!(!workspace.directory.path().join("would-create.txt").exists());
}

#[tokio::test]
async fn blank_context_and_crlf_are_preserved() {
    let workspace = Workspace::new(&[("file.txt", "before\r\n\r\nold\r\nafter\r\n")]);
    let report = workspace.patch("*** Begin Patch\n*** Update File: file.txt\n@@\n before\n\n-old\n+new\n after\n*** End Patch").await;
    assert!(report.starts_with("Patch applied successfully"), "{report}");
    assert_eq!(workspace.read("file.txt"), "before\r\n\r\nnew\r\nafter\r\n");
}

#[tokio::test]
async fn consecutive_navigation_anchors_select_a_nested_block() {
    let workspace = Workspace::new(&[(
        "file.txt",
        "class A\n  method one\n    value\n  method two\n    value\nclass B\n  method two\n    value\n",
    )]);
    let report = workspace.patch("*** Begin Patch\n*** Update File: file.txt\n@@ class A\n@@   method two\n-    value\n+    chosen\n*** End Patch").await;
    assert!(report.starts_with("Patch applied successfully"), "{report}");
    assert_eq!(
        workspace.read("file.txt"),
        "class A\n  method one\n    value\n  method two\n    chosen\nclass B\n  method two\n    value\n"
    );
}

#[tokio::test]
async fn edits_preserve_trailing_blank_lines_and_can_append_one() {
    for ending in ["\n", "\r\n"] {
        let workspace = Workspace::new(&[("one.txt", &format!("old{ending}{ending}{ending}"))]);
        let report = workspace
            .patch("*** Begin Patch\n*** Update File: one.txt\n@@\n-old\n+new\n*** End Patch")
            .await;
        assert!(report.contains("Modified"), "{report}");
        assert_eq!(
            workspace.read("one.txt"),
            format!("new{ending}{ending}{ending}")
        );
    }
    let workspace = Workspace::new(&[("one.txt", "old\n")]);
    workspace
        .patch("*** Begin Patch\n*** Update File: one.txt\n@@\n old\n+\n*** End Patch")
        .await;
    assert_eq!(workspace.read("one.txt"), "old\n\n");
}

#[tokio::test]
async fn text_edits_leave_non_utf8_files_unchanged() {
    let workspace = Workspace::new(&[]);
    let bytes = b"old\n\xff\n";
    let path = workspace.directory.path().join("binary.txt");
    std::fs::write(&path, bytes).unwrap();
    let report = workspace
        .patch("*** Begin Patch\n*** Update File: binary.txt\n@@\n-old\n+new\n*** End Patch")
        .await;
    assert!(report.contains("Not applied"), "{report}");
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let output = swiftide_agents_contrib::tools::ExactReplace.invoke(
        &workspace.context,
        &ToolCall::builder().id("edit").name("edit")
            .args(serde_json::json!({"file_path":"binary.txt", "old_string":"old", "new_string":"new"}).to_string())
            .build().unwrap(),
    ).await.unwrap();
    assert!(output.as_fail().unwrap().contains("UTF-8"));
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[cfg(unix)]
#[tokio::test]
async fn moving_to_a_filesystem_alias_does_not_write_or_remove_the_source() {
    use std::os::unix::fs::symlink;
    let workspace = Workspace::new(&[("one.txt", "old\n"), ("nested/one.txt", "nested\n")]);
    symlink("one.txt", workspace.directory.path().join("link.txt")).unwrap();
    std::fs::hard_link(
        workspace.directory.path().join("one.txt"),
        workspace.directory.path().join("hard.txt"),
    )
    .unwrap();
    symlink("nested", workspace.directory.path().join("alias")).unwrap();
    for (source, destination, expected) in [
        ("one.txt", "link.txt", "old\n"),
        ("link.txt", "one.txt", "old\n"),
        ("one.txt", "hard.txt", "old\n"),
        ("nested/one.txt", "alias/one.txt", "nested\n"),
    ] {
        let report = workspace.patch(&format!("*** Begin Patch\n*** Update File: {source}\n*** Move to: {destination}\n@@\n-{}\n+new\n*** End Patch", expected.trim())).await;
        assert!(report.contains("destination must differ"), "{report}");
        assert_eq!(workspace.read(source), expected);
        assert_eq!(workspace.read(destination), expected);
    }
}
