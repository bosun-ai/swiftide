#![cfg(feature = "file-tools")]

use std::sync::Arc;

use serde_json::{Value, json};
use swiftide_agents::{DefaultContext, tools::local_executor::LocalExecutor};
use swiftide_agents_contrib::tools::ReadFile;
use swiftide_core::{
    ToolExecutor,
    chat_completion::{Tool, ToolCall, ToolOutput},
};

async fn invoke(tool: &ReadFile, context: &DefaultContext, args: Value) -> ToolOutput {
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
    .unwrap()
}

#[tokio::test]
async fn both_source_read_modes_page_through_executor_output_with_original_line_numbers() {
    let dir = temp_dir::TempDir::new().unwrap();
    let path = dir.path().join("notes.txt");
    std::fs::write(&path, "line 1\nline 2\nline 3\n").unwrap();
    let context = DefaultContext::from_executor(Arc::new(
        LocalExecutor::builder()
            .workdir(dir.path())
            .build()
            .unwrap(),
    ) as Arc<dyn ToolExecutor>);
    let plain = ReadFile::default();
    let numbered = plain.clone().with_line_numbers();
    assert_eq!(
        invoke(&plain, &context, json!({"file_name":"notes.txt"}))
            .await
            .as_text(),
        Some("line 1\nline 2\nline 3\n")
    );
    assert_eq!(
        invoke(
            &numbered,
            &context,
            json!({"file_name":"notes.txt","offset":2,"limit":1})
        )
        .await
        .as_text(),
        Some("2|line 2\n[File has 1 more lines. Use offset=3 to continue.]")
    );
    assert_eq!(
        invoke(
            &plain,
            &context,
            json!({"file_name":path,"offset":0,"limit":1})
        )
        .await
        .as_text(),
        Some("line 1\n[File has 2 more lines. Use offset=2 to continue.]")
    );
    for tool in [&plain, &numbered] {
        assert_eq!(
            invoke(tool, &context, json!({"file_name":"notes.txt","limit":0}))
                .await
                .as_fail(),
            Some("limit must be 1 or greater")
        );
        assert!(
            invoke(tool, &context, json!({"file_name":"missing.txt"}))
                .await
                .as_text()
                .unwrap()
                .contains("missing.txt")
        );
        // The source derive macro accepts extra keys at runtime despite its strict schema.
        assert!(
            invoke(
                tool,
                &context,
                json!({"file_name":"notes.txt","extra":true})
            )
            .await
            .as_text()
            .is_some()
        );
        let call = ToolCall::builder()
            .id("missing-args")
            .name(tool.name())
            .build()
            .unwrap();
        let error = tool.invoke(&context, &call).await.unwrap_err();
        assert!(
            matches!(error, swiftide_core::chat_completion::errors::ToolError::MissingArguments(message) if message == format!("No arguments provided for {}",tool.name()))
        );
    }
}

#[tokio::test]
async fn both_source_read_modes_apply_default_paging_line_truncation_and_window_refusal() {
    let dir = temp_dir::TempDir::new().unwrap();
    let path = dir.path().join("notes.txt");
    let context =
        DefaultContext::from_executor(Arc::new(LocalExecutor::default()) as Arc<dyn ToolExecutor>);
    for tool in [ReadFile::default(), ReadFile::default().with_line_numbers()] {
        std::fs::write(&path, "small\n".repeat(2_500)).unwrap();
        let output = invoke(&tool, &context, json!({"file_name":path})).await;
        assert!(
            output
                .as_text()
                .unwrap()
                .ends_with("[File has 500 more lines. Use offset=2001 to continue.]")
        );
        std::fs::write(&path, "é".repeat(2_005)).unwrap();
        let output = invoke(&tool, &context, json!({"file_name":path})).await;
        assert!(output.as_text().unwrap().ends_with("… [line truncated]"));
        assert_eq!(output.as_text().unwrap().matches('é').count(), 2_000);
        std::fs::write(&path, format!("{}\n", "x".repeat(1_000)).repeat(150)).unwrap();
        assert!(
            invoke(&tool, &context, json!({"file_name":path}))
                .await
                .as_fail()
                .unwrap()
                .contains("exceeds the maximum of 25000 tokens")
        );
        assert!(
            invoke(&tool, &context, json!({"file_name":path,"limit":50}))
                .await
                .as_text()
                .is_some()
        );
    }
}

#[test]
fn read_specs_preserve_the_source_macro_contracts() {
    let expected: Value = serde_json::from_str(include_str!("fixtures/read_spec.json")).unwrap();
    let actual: Vec<_> = [ReadFile::default(), ReadFile::default().with_line_numbers()]
        .iter()
        .map(Tool::tool_spec)
        .collect();
    assert_eq!(serde_json::to_value(actual).unwrap(), expected);
}
