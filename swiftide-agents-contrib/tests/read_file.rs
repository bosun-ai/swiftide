#![cfg(feature = "file-tools")]
use serde_json::{Value, json};
use std::{path::Path, sync::Arc};
use swiftide_agents::{DefaultContext, tools::local_executor::LocalExecutor};
use swiftide_agents_contrib::tools::ReadFile;
use swiftide_core::{
    Command, CommandError, CommandOutput, ToolExecutor,
    chat_completion::{Tool, ToolCall, ToolOutput},
    indexing::IndexingStream,
};
fn context(directory: &Path) -> DefaultContext {
    DefaultContext::from_executor(Arc::new(
        LocalExecutor::builder().workdir(directory).build().unwrap(),
    ) as Arc<dyn ToolExecutor>)
}
async fn read(tool: &ReadFile, context: &DefaultContext, args: Value) -> ToolOutput {
    tool.invoke(
        context,
        &ToolCall::builder()
            .id("read")
            .name("Read")
            .args(args.to_string())
            .build()
            .unwrap(),
    )
    .await
    .unwrap()
}
#[tokio::test]
async fn source_read_arguments_numbering_empty_files_and_past_end_messages_are_preserved() {
    let directory = temp_dir::TempDir::new().unwrap();
    let path = directory.path().join("a file's.txt");
    std::fs::write(&path, "one\ntwo\nthree").unwrap();
    let context = context(directory.path());
    let tool = ReadFile::default();
    for (offset, limit, expected) in [
        (None, None, "     1→one\n     2→two\n     3→three"),
        (Some(0), Some(1), "     1→one"),
        (Some(2), Some(2), "     2→two\n     3→three"),
        (
            Some(30),
            None,
            "The file has 3 lines; offset 30 is past the end of the file.",
        ),
    ] {
        assert_eq!(
            read(
                &tool,
                &context,
                json!({"file_path":path,"offset":offset,"limit":limit})
            )
            .await
            .as_text(),
            Some(expected)
        );
    }
    assert!(
        read(&tool, &context, json!({"file_path":"relative.txt"}))
            .await
            .as_fail()
            .is_some()
    );
    assert!(
        read(&tool, &context, json!({"file_path":path,"limit":0}))
            .await
            .as_fail()
            .is_some()
    );
    assert!(
        read(&tool, &context, json!({"file_path":path,"limit":100_001}))
            .await
            .as_fail()
            .is_some()
    );
    std::fs::write(&path, "").unwrap();
    assert_eq!(
        read(&tool, &context, json!({"file_path":path}))
            .await
            .as_text(),
        Some("File exists but has empty contents.")
    );
}
#[tokio::test]
async fn source_default_line_limit_continuation_and_character_accounting_are_preserved() {
    use std::fmt::Write as _;
    let directory = temp_dir::TempDir::new().unwrap();
    let path = directory.path().join("large.txt");
    let context = context(directory.path());
    let tool = ReadFile::default();
    let mut text = String::new();
    for line in 1..=2001 {
        writeln!(text, "line {line}").unwrap();
    }
    std::fs::write(&path, text).unwrap();
    let output = read(&tool, &context, json!({"file_path":path})).await;
    assert!(
        output
            .as_text()
            .unwrap()
            .contains("PARTIAL view: showing lines 1-2000")
    );
    assert!(output.as_text().unwrap().contains("offset=2001"));
    assert!(!output.as_text().unwrap().contains("line 2001"));
    let next = read(&tool, &context, json!({"file_path":path,"offset":2001})).await;
    assert_eq!(next.as_text(), Some("  2001→line 2001"));
    std::fs::write(&path, format!("{}\n", "x".repeat(100)).repeat(1000)).unwrap();
    let output = read(&tool, &context, json!({"file_path":path})).await;
    // The source budget reserves 16 characters per line; it is not the rendered byte count.
    assert!(output.as_text().unwrap().contains("showing lines 1-862"));
    assert!(output.as_text().unwrap().contains("offset=863"));
    assert!(
        read(&tool, &context, json!({"file_path":path,"limit":1000}))
            .await
            .as_fail()
            .is_some()
    );
    std::fs::write(&path, "x".repeat(100_001)).unwrap();
    assert!(
        read(&tool, &context, json!({"file_path":path}))
            .await
            .as_fail()
            .is_some()
    );
}
#[derive(Clone)]
struct ChangeDuringRead {
    executor: LocalExecutor,
    path: std::path::PathBuf,
}
#[async_trait::async_trait]
impl ToolExecutor for ChangeDuringRead {
    async fn exec_cmd(&self, command: &Command) -> Result<CommandOutput, CommandError> {
        let result = self.executor.exec_cmd(command).await?;
        if matches!(command,Command::Shell{command,..} if command.contains("__QUAK_CLAUDE_READ_META__"))
        {
            std::fs::write(&self.path, "changed after the read").unwrap();
        }
        Ok(result)
    }
    async fn stream_files(
        &self,
        path: &Path,
        extensions: Option<Vec<String>>,
    ) -> anyhow::Result<IndexingStream<String>> {
        self.executor.stream_files(path, extensions).await
    }
}
#[tokio::test]
async fn source_metadata_check_rejects_a_file_changed_during_reading() {
    let directory = temp_dir::TempDir::new().unwrap();
    let path = directory.path().join("file.txt");
    std::fs::write(&path, "old").unwrap();
    let context = DefaultContext::from_executor(Arc::new(ChangeDuringRead {
        executor: LocalExecutor::builder()
            .workdir(directory.path())
            .build()
            .unwrap(),
        path: path.clone(),
    }) as Arc<dyn ToolExecutor>);
    assert!(
        read(&ReadFile::default(), &context, json!({"file_path":path}))
            .await
            .as_fail()
            .unwrap()
            .contains("changed while it was being read")
    );
}

#[test]
fn read_spec_matches_the_source_snapshot() {
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/read_spec.json")).unwrap();
    assert_eq!(
        serde_json::to_value(ReadFile::default().tool_spec()).unwrap(),
        expected
    );
}
