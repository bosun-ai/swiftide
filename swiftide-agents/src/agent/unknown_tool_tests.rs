use std::sync::Mutex;

use super::*;
use crate::{test_utils::MockTool, tools::control::ApprovalRequired};
use swiftide_core::{chat_completion::errors::ToolError, test_utils::MockChatCompletion};

fn calls(names: &[&str], attempt: usize) -> Vec<ToolCall> {
    names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            ToolCall::builder()
                .name(*name)
                .id(format!("call_{attempt}_{index}"))
                .args("{}")
                .build()
                .unwrap()
        })
        .collect()
}

#[tokio::test]
async fn unknown_tools_use_ordered_results_and_hooks() {
    let tool = MockTool::new("shell_command");
    for _ in 0..2 {
        tool.expect_invoke_ok(ToolOutput::text("executed"), Some("{}"));
    }
    let before = Arc::new(Mutex::new(Vec::new()));
    let before_hook = before.clone();
    let after = Arc::new(Mutex::new(Vec::new()));
    let after_hook = after.clone();
    let mut agent = Agent::builder()
        .llm(&MockChatCompletion::new())
        .tools([tool])
        .before_tool(move |_: &mut Agent, call: &ToolCall| {
            before_hook.lock().unwrap().push(call.id().to_owned());
            Box::pin(async { Ok(()) })
        })
        .after_tool(
            move |_: &mut Agent, call: &ToolCall, output: &mut Result<ToolOutput, ToolError>| {
                after_hook
                    .lock()
                    .unwrap()
                    .push((call.id().to_owned(), output.is_err()));
                Box::pin(async { Ok(()) })
            },
        )
        .build()
        .unwrap();
    let calls = calls(
        &["shell_command", "bash", "shell_command", "unavailable"],
        0,
    );

    agent.invoke_tools(&calls).await.unwrap();

    let expected_ids = calls
        .iter()
        .map(|call| call.id().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(*before.lock().unwrap(), expected_ids);
    assert_eq!(
        *after.lock().unwrap(),
        expected_ids
            .iter()
            .enumerate()
            .map(|(index, id)| (id.clone(), index % 2 == 1))
            .collect::<Vec<_>>()
    );
    let history = agent.history().await.unwrap();
    assert_eq!(history.len(), calls.len());
    for (message, call) in history.iter().zip(&calls) {
        let ChatMessage::ToolOutput(recorded, output) = message else {
            panic!("expected a tool result");
        };
        assert_eq!(recorded, call);
        if call.name() == "shell_command" {
            assert_eq!(output.as_text(), Some("executed"));
        } else {
            assert_eq!(
                output.as_fail(),
                Some(format!("Unknown tool '{}'. Use an available tool.", call.name()).as_str())
            );
        }
    }
}

#[tokio::test]
async fn unknown_tool_retries_ignore_call_ids_and_finish_the_batch() {
    for limit in [0, 1, 3] {
        let tool = MockTool::new("shell_command");
        for _ in 0..=limit {
            tool.expect_invoke_ok(ToolOutput::text("executed"), Some("{}"));
        }
        let mut agent = Agent::builder()
            .llm(&MockChatCompletion::new())
            .tools([tool])
            .tool_retry_limit(limit)
            .build()
            .unwrap();

        for attempt in 0..=limit {
            let calls = calls(&["bash", "shell_command"], attempt);
            let result = agent.invoke_tools(&calls).await;
            let history = agent.history().await.unwrap();
            assert_eq!(history.len(), 2 * (attempt + 1));
            for (message, call) in history[history.len() - 2..].iter().zip(&calls) {
                let ChatMessage::ToolOutput(recorded, _) = message else {
                    panic!("expected a tool result");
                };
                assert_eq!(recorded, call);
            }
            if attempt < limit {
                result.unwrap();
                assert!(agent.stop_reason().is_none());
            } else {
                assert!(matches!(
                    result,
                    Err(AgentError::ToolError(ToolError::Unknown(_)))
                ));
                assert_eq!(
                    agent.stop_reason().unwrap().as_tool_calls_over_limit(),
                    Some(&calls[0])
                );
            }
        }
    }
}

#[tokio::test]
async fn unknown_tools_preserve_approval_suspension() {
    // There are no expectations: the wrapped tool must never execute without approval.
    let tool = ApprovalRequired::new(MockTool::new("shell_command"));
    let mut agent = Agent::builder()
        .llm(&MockChatCompletion::new())
        .tools([tool])
        .build()
        .unwrap();
    let calls = calls(&["bash", "shell_command", "unavailable"], 0);

    agent.invoke_tools(&calls).await.unwrap();

    let (pending, _) = agent.stop_reason().unwrap().as_feedback_required().unwrap();
    assert_eq!(pending, &calls[1]);
    let history = agent.history().await.unwrap();
    assert_eq!(history.len(), 2);
    for (message, call) in history.iter().zip([&calls[0], &calls[2]]) {
        let ChatMessage::ToolOutput(recorded, ToolOutput::Fail(_)) = message else {
            panic!("only unavailable tools should have results while approval is pending");
        };
        assert_eq!(recorded, call);
    }
}

#[tokio::test]
async fn unknown_tools_preserve_stop_results() {
    let mut agent = Agent::builder()
        .llm(&MockChatCompletion::new())
        .build()
        .unwrap();
    let calls = calls(&["bash", "stop", "unavailable"], 0);

    agent.invoke_tools(&calls).await.unwrap();

    let (stopped_by, _) = agent.stop_reason().unwrap().as_requested_by_tool().unwrap();
    assert_eq!(stopped_by, &calls[1]);
    let history = agent.history().await.unwrap();
    assert_eq!(history.len(), calls.len());
    assert!(
        matches!(&history[1], ChatMessage::ToolOutput(call, ToolOutput::Stop(_)) if call == &calls[1])
    );
}
