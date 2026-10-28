#![cfg(feature = "file-tools")]

use std::sync::Arc;

use serde_json::{Value, json};
use swiftide_agents::{DefaultContext, tools::local_executor::LocalExecutor};
use swiftide_agents_contrib::tools::ReadFile;
use swiftide_core::{
    AgentContext, Command, CommandError, CommandOutput, ToolExecutor,
    chat_completion::{Tool, ToolCall, ToolOutput, errors::ToolError},
    indexing::IndexingStream,
};

async fn read(
    tool: &ReadFile,
    context: &dyn AgentContext,
    args: Value,
) -> Result<ToolOutput, ToolError> {
    tool.invoke(
        context,
        &ToolCall::builder()
            .id("read")
            .name(tool.name())
            .args(args.to_string())
            .build()
            .unwrap(),
    )
    .await
}

fn context(directory: &std::path::Path) -> DefaultContext {
    let executor: Arc<dyn ToolExecutor> =
        Arc::new(LocalExecutor::builder().workdir(directory).build().unwrap());
    DefaultContext::from_executor(executor)
}

#[tokio::test]
async fn full_reads_preserve_contents_and_numbered_reads_keep_original_line_numbers() {
    let directory = temp_dir::TempDir::new().unwrap();
    std::fs::write(directory.path().join("-notes"), "one\ntwo\nthree").unwrap();
    let context = context(directory.path());
    for (start, end, numbered, expected) in [
        (None, None, false, "one\ntwo\nthree"),
        (None, None, true, "1|one\n2|two\n3|three\n"),
        (Some(1), None, false, "1|one\n2|two\n3|three\n"),
        (Some(2), Some(3), false, "2|two\n3|three\n"),
        (Some(2), None, false, "2|two\n3|three\n"),
        (None, Some(2), false, "1|one\n2|two\n"),
        (Some(3), Some(30), false, "3|three\n"),
        (Some(30), None, false, ""),
    ] {
        let tool = if numbered {
            ReadFile::default().with_line_numbers()
        } else {
            ReadFile::default()
        };
        let result = read(
            &tool,
            &context,
            json!({"file_name":"-notes", "start_line":start, "end_line":end}),
        )
        .await
        .unwrap();
        assert_eq!(
            result.content(),
            Some(expected),
            "{start:?} {end:?} {numbered}"
        );
    }
}

#[tokio::test]
async fn ranges_work_with_absolute_paths_and_shell_special_characters() {
    let directory = temp_dir::TempDir::new().unwrap();
    let context = context(directory.path());
    for name in ["a file's.rs", "key=value", "$(touch should-not-exist)"] {
        let path = directory.path().join(name);
        std::fs::write(&path, "one\ntwo\nthree\nfour\n").unwrap();
        for file_name in [name.to_owned(), path.to_string_lossy().into_owned()] {
            let result = read(
                &ReadFile::default(),
                &context,
                json!({
                    "file_name":file_name, "start_line":2, "end_line":3
                }),
            )
            .await
            .unwrap();
            assert_eq!(result.content(), Some("2|two\n3|three\n"));
        }
    }
    assert!(!directory.path().join("should-not-exist").exists());
}

#[tokio::test]
async fn empty_files_and_ranges_past_the_end_return_empty_text() {
    let directory = temp_dir::TempDir::new().unwrap();
    std::fs::write(directory.path().join("empty"), "").unwrap();
    let context = context(directory.path());
    for tool in [ReadFile::default(), ReadFile::default().with_line_numbers()] {
        for args in [
            json!({"file_name":"empty"}),
            json!({"file_name":"empty","start_line":5}),
        ] {
            assert_eq!(
                read(&tool, &context, args).await.unwrap().content(),
                Some("")
            );
        }
    }
}

#[tokio::test]
async fn default_pages_stop_after_two_thousand_lines_and_explain_how_to_continue() {
    let directory = temp_dir::TempDir::new().unwrap();
    use std::fmt::Write as _;
    let mut contents = String::new();
    for line in 1..=2_001 {
        writeln!(contents, "line {line}").unwrap();
    }
    std::fs::write(directory.path().join("large.txt"), &contents).unwrap();
    let context = context(directory.path());
    for tool in [ReadFile::default(), ReadFile::default().with_line_numbers()] {
        let result = read(&tool, &context, json!({"file_name":"large.txt"}))
            .await
            .unwrap();
        let output = result.as_text().unwrap();
        assert!(output.contains("line 2000\n"));
        assert!(!output.contains("line 2001\n"));
        assert!(output.contains("PARTIAL view"));
        assert!(output.contains("start_line=2001"));
        let next = read(
            &tool,
            &context,
            json!({"file_name":"large.txt","start_line":2001}),
        )
        .await
        .unwrap();
        assert_eq!(next.as_text(), Some("2001|line 2001\n"));
    }
    let result = read(
        &ReadFile::default(),
        &context,
        json!({"file_name":"large.txt","end_line":2001}),
    )
    .await
    .unwrap();
    assert!(result.as_text().unwrap().contains("2001|line 2001\n"));
    assert!(!result.as_text().unwrap().contains("PARTIAL"));
}

#[tokio::test]
async fn text_budget_marks_default_pages_and_rejects_oversized_ranges_or_single_lines() {
    let directory = temp_dir::TempDir::new().unwrap();
    let line = format!("{}\n", "α".repeat(10_000));
    std::fs::write(directory.path().join("large.txt"), line.repeat(6)).unwrap();
    std::fs::write(directory.path().join("long.txt"), "x".repeat(100_001)).unwrap();
    let context = context(directory.path());
    let result = read(
        &ReadFile::default(),
        &context,
        json!({"file_name":"large.txt"}),
    )
    .await
    .unwrap();
    let output = result.as_text().unwrap();
    assert!(output.contains("PARTIAL view"));
    assert!(output.contains("start_line=5"));
    assert!(!output.contains('\u{fffd}'));
    assert!(output.len() < 100_200);
    for args in [
        json!({"file_name":"large.txt","end_line":6}),
        json!({"file_name":"long.txt"}),
    ] {
        assert!(
            read(&ReadFile::default(), &context, args)
                .await
                .unwrap()
                .as_fail()
                .is_some()
        );
    }
    // Bytes beyond a requested range must not make a valid small excerpt fail.
    std::fs::write(
        directory.path().join("prefix.txt"),
        format!("short\n{}", "x".repeat(100_001)),
    )
    .unwrap();
    let result = read(
        &ReadFile::default(),
        &context,
        json!({"file_name":"prefix.txt","end_line":1}),
    )
    .await
    .unwrap();
    assert_eq!(result.as_text(), Some("1|short\n"));
}

#[derive(Clone)]
struct RecordingExecutor {
    inner: Arc<dyn ToolExecutor>,
    commands: Arc<std::sync::Mutex<Vec<Command>>>,
}

#[async_trait::async_trait]
impl ToolExecutor for RecordingExecutor {
    async fn exec_cmd(&self, command: &Command) -> Result<CommandOutput, CommandError> {
        self.commands.lock().unwrap().push(command.clone());
        self.inner.exec_cmd(command).await
    }

    async fn stream_files(
        &self,
        path: &std::path::Path,
        extensions: Option<Vec<String>>,
    ) -> anyhow::Result<IndexingStream<String>> {
        self.inner.stream_files(path, extensions).await
    }
}

#[tokio::test]
async fn excerpts_select_lines_inside_the_executor_without_reading_the_whole_file_first() {
    let directory = temp_dir::TempDir::new().unwrap();
    std::fs::write(directory.path().join("file.txt"), "one\ntwo\nthree\nfour\n").unwrap();
    let commands = Arc::new(std::sync::Mutex::new(Vec::new()));
    let executor: Arc<dyn ToolExecutor> = Arc::new(RecordingExecutor {
        inner: Arc::new(
            LocalExecutor::builder()
                .workdir(directory.path())
                .build()
                .unwrap(),
        ),
        commands: Arc::clone(&commands),
    });
    let context = DefaultContext::from_executor(executor);
    let result = read(
        &ReadFile::default(),
        &context,
        json!({"file_name":"file.txt","start_line":2,"end_line":3}),
    )
    .await
    .unwrap();
    assert_eq!(result.content(), Some("2|two\n3|three\n"));
    let commands = commands.lock().unwrap();
    assert_eq!(commands.len(), 1);
    assert!(
        matches!(&commands[0], Command::Shell { command, .. } if command.contains("tail -n +2 | head -n 3 | head -c 100001"))
    );
}

#[tokio::test]
async fn invalid_ranges_fail_before_touching_the_executor_and_file_errors_remain_visible() {
    let directory = temp_dir::TempDir::new().unwrap();
    let commands = Arc::new(std::sync::Mutex::new(Vec::new()));
    let executor: Arc<dyn ToolExecutor> = Arc::new(RecordingExecutor {
        inner: Arc::new(
            LocalExecutor::builder()
                .workdir(directory.path())
                .build()
                .unwrap(),
        ),
        commands: Arc::clone(&commands),
    });
    let context = DefaultContext::from_executor(executor);
    for args in [
        json!({"file_name":"missing","start_line":0}),
        json!({"file_name":"missing","end_line":0}),
        json!({"file_name":"missing","start_line":-1}),
        json!({"file_name":"missing","start_line":4,"end_line":2}),
        json!({"file_name":"missing","unexpected":1}),
        json!({"file_name":"missing","end_line":100_001}),
    ] {
        let result = read(&ReadFile::default(), &context, args).await;
        assert!(result.is_err() || result.unwrap().as_fail().is_some());
        assert!(commands.lock().unwrap().is_empty());
    }
    for args in [
        json!({"file_name":"missing"}),
        json!({"file_name":"missing","start_line":2}),
        json!({"file_name":"."}),
    ] {
        assert!(read(&ReadFile::default(), &context, args).await.is_err());
    }
}

#[test]
fn both_reading_modes_publish_valid_schemas_with_positive_line_numbers() {
    for tool in [ReadFile::default(), ReadFile::default().with_line_numbers()] {
        let spec = tool.tool_spec();
        spec.strict_parameters_schema().unwrap();
        let schema = serde_json::to_value(spec.parameters_schema.unwrap()).unwrap();
        assert_eq!(schema["required"], json!(["file_name"]));
        assert!(
            schema["properties"]["start_line"]
                .to_string()
                .contains("minimum")
        );
        assert!(
            schema["properties"]["end_line"]
                .to_string()
                .contains("minimum")
        );
    }
    assert_eq!(ReadFile::default().name(), "read_file");
    assert_eq!(
        ReadFile::default().with_line_numbers().name(),
        "read_file_with_line_numbers"
    );
}
