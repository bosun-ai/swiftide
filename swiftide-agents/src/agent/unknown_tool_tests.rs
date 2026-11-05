use std::sync::Mutex;

use super::*;
use crate::{test_utils::MockTool, tools::control::ApprovalRequired};
use swiftide_core::{
    ToolFeedback, chat_completion::errors::ToolError, test_utils::MockChatCompletion,
};

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

#[test]
fn pending_calls_match_ids_only_after_the_latest_assistant_message() {
    let calls = calls(&["shell_command", "shell_command"], 0);
    let mut history = vec![
        // An earlier result with a reused ID cannot answer a later call.
        ChatMessage::Assistant(None, Some(vec![calls[1].clone()])),
        ChatMessage::ToolOutput(calls[1].clone(), ToolOutput::text("earlier batch")),
        ChatMessage::Assistant(None, Some(calls.clone())),
        ChatMessage::ToolOutput(calls[0].clone(), ToolOutput::text("completed")),
    ];
    assert_eq!(pending_tool_calls(&history), vec![calls[1].clone()]);

    history.push(ChatMessage::ToolOutput(
        calls[1].clone(),
        ToolOutput::text("done"),
    ));
    assert!(pending_tool_calls(&history).is_empty());

    history.push(ChatMessage::Assistant(None, Some(calls)));
    assert_eq!(pending_tool_calls(&history).len(), 2);
    history.push(ChatMessage::Assistant(Some("Done".into()), None));
    assert!(pending_tool_calls(&history).is_empty());
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
async fn retry_limits_preserve_control_results_in_either_order() {
    for output in [
        ToolOutput::Stop(None),
        ToolOutput::FeedbackRequired(None),
        ToolOutput::AgentFailed(None),
    ] {
        for limit in [0, 1, 3] {
            for names in [["bash", "control"], ["control", "bash"]] {
                let tool = MockTool::new("control");
                tool.expect_invoke_ok(output.clone(), Some("{}"));
                let stopped = Arc::new(Mutex::new(Vec::new()));
                let stop_hook = stopped.clone();
                let mut agent = Agent::builder()
                    .llm(&MockChatCompletion::new())
                    .tools([tool])
                    .tool_retry_limit(limit)
                    .on_stop(
                        move |_: &mut Agent, reason: StopReason, _: Option<&AgentError>| {
                            stop_hook.lock().unwrap().push(reason);
                            Box::pin(async { Ok(()) })
                        },
                    )
                    .build()
                    .unwrap();
                for attempt in 0..limit {
                    agent
                        .invoke_tools(&calls(&["bash"], attempt))
                        .await
                        .unwrap();
                }
                let calls = calls(&names, limit);
                let control = calls.iter().find(|call| call.name() == "control").unwrap();

                agent.invoke_tools(&calls).await.unwrap();

                let expected_reason = match &output {
                    ToolOutput::Stop(_) => StopReason::RequestedByTool(control.clone(), None),
                    ToolOutput::FeedbackRequired(_) => StopReason::FeedbackRequired {
                        tool_call: control.clone(),
                        payload: None,
                    },
                    ToolOutput::AgentFailed(_) => StopReason::AgentFailed(None),
                    _ => unreachable!(),
                };
                assert_eq!(agent.stop_reason(), Some(&expected_reason));
                assert_eq!(*stopped.lock().unwrap(), vec![expected_reason]);
                let history = agent.history().await.unwrap();
                let expected_calls = calls
                    .iter()
                    .filter(|call| call.name() != "control" || !output.is_feedback_required())
                    .collect::<Vec<_>>();
                assert_eq!(history.len(), limit + expected_calls.len());
                for (message, call) in history[limit..].iter().zip(expected_calls) {
                    let ChatMessage::ToolOutput(recorded, result) = message else {
                        panic!("expected a tool result");
                    };
                    assert_eq!(recorded, call);
                    if call.name() == "control" {
                        assert_eq!(result, &output);
                    } else {
                        assert!(result.is_fail());
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn retry_limits_still_return_errors_when_resuming_a_stopped_agent() {
    let mut agent = Agent::builder()
        .llm(&MockChatCompletion::new())
        .tool_retry_limit(0)
        .build()
        .unwrap();
    agent.stop(StopReason::Other("previous run".into())).await;
    let calls = calls(&["bash"], 0);
    agent
        .add_message(ChatMessage::Assistant(None, Some(calls.clone())))
        .await
        .unwrap();

    assert!(matches!(
        agent.run().await,
        Err(AgentError::ToolError(ToolError::Unknown(_)))
    ));
    let history = agent.history().await.unwrap();
    assert!(
        matches!(history.last(), Some(ChatMessage::ToolOutput(call, ToolOutput::Fail(_))) if call == &calls[0])
    );
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

    agent
        .add_message(ChatMessage::Assistant(None, Some(calls.clone())))
        .await
        .unwrap();
    agent.invoke_tools(&calls).await.unwrap();

    let (pending, _) = agent.stop_reason().unwrap().as_feedback_required().unwrap();
    assert_eq!(pending, &calls[1]);
    let history = agent.history().await.unwrap();
    assert_eq!(history.len(), 3);
    for (message, call) in history[1..].iter().zip([&calls[0], &calls[2]]) {
        let ChatMessage::ToolOutput(recorded, ToolOutput::Fail(_)) = message else {
            panic!("only unavailable tools should have results while approval is pending");
        };
        assert_eq!(recorded, call);
    }

    assert!(matches!(
        agent.run_once().await,
        Err(AgentError::UnfulfilledToolCalls)
    ));
}

#[tokio::test]
async fn completion_loop_resumes_only_unanswered_calls() {
    let calls = calls(&["shell_command", "shell_command"], 0);
    let restored = vec![
        ChatMessage::Assistant(None, Some(calls.clone())),
        ChatMessage::ToolOutput(calls[0].clone(), ToolOutput::text("already completed")),
    ];
    let mut expected = restored.clone();
    expected.push(ChatMessage::ToolOutput(
        calls[1].clone(),
        ToolOutput::text("resumed once"),
    ));
    let tool = MockTool::new("shell_command");
    tool.expect_invoke_ok(ToolOutput::text("resumed once"), Some("{}"));
    let llm = MockChatCompletion::new();
    let mut agent = Agent::builder()
        .llm(&llm)
        .tools([tool])
        .on_start(move |agent: &mut Agent| {
            let restored = restored.clone();
            Box::pin(async move { agent.context().add_messages(restored).await })
        })
        .build()
        .unwrap();
    llm.expect_complete(
        ChatCompletionRequest::builder()
            .messages(expected)
            .tool_specs(agent.tools().iter().map(swiftide_core::Tool::tool_spec))
            .build()
            .unwrap(),
        Ok(
            swiftide_core::chat_completion::ChatCompletionResponse::builder()
                .message("Done")
                .build()
                .unwrap(),
        ),
    );

    agent.run_once().await.unwrap();
    assert_eq!(llm.received_expectations.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn mixed_batches_resume_only_unanswered_approval_calls() {
    for names in [
        ["approved", "bash", "shell_command"],
        ["bash", "approved", "shell_command"],
        ["bash", "shell_command", "approved"],
    ] {
        for approved in [false, true] {
            for new_query in [None, Some("Continue")] {
                let known = MockTool::new("shell_command");
                known.expect_invoke_ok(ToolOutput::text("executed once"), Some("{}"));
                let pending = MockTool::new("approved");
                if approved {
                    pending.expect_invoke_ok(ToolOutput::text("approved once"), Some("{}"));
                }
                let llm = MockChatCompletion::new();
                let mut agent = Agent::builder()
                    .llm(&llm)
                    .tools([known.boxed(), ApprovalRequired::new(pending).boxed()])
                    .tool_retry_limit(0)
                    .build()
                    .unwrap();
                let calls = calls(&names, 0);
                agent
                    .add_message(ChatMessage::Assistant(None, Some(calls.clone())))
                    .await
                    .unwrap();
                agent.invoke_tools(&calls).await.unwrap();
                let (pending, _) = agent.stop_reason().unwrap().as_feedback_required().unwrap();
                let pending = pending.clone();
                let feedback = if approved {
                    ToolFeedback::approved()
                } else {
                    ToolFeedback::refused()
                };
                agent
                    .context()
                    .feedback_received(&pending, &feedback)
                    .await
                    .unwrap();

                let mut expected = agent.history().await.unwrap();
                assert_eq!(expected.len(), 3);
                expected.push(ChatMessage::ToolOutput(
                    pending,
                    ToolOutput::text(if approved {
                        "approved once"
                    } else {
                        "This tool call was refused"
                    }),
                ));
                if let Some(query) = new_query {
                    expected.push(ChatMessage::new_user(query));
                }
                llm.expect_complete(
                    ChatCompletionRequest::builder()
                        .messages(expected)
                        .tool_specs(agent.tools().iter().map(swiftide_core::Tool::tool_spec))
                        .build()
                        .unwrap(),
                    Ok(
                        swiftide_core::chat_completion::ChatCompletionResponse::builder()
                            .message("Done")
                            .build()
                            .unwrap(),
                    ),
                );

                if let Some(query) = new_query {
                    agent.query_once(query).await.unwrap();
                } else {
                    agent.run_once().await.unwrap();
                }
                assert_eq!(llm.received_expectations.lock().unwrap().len(), 1);
                let history = agent.history().await.unwrap();
                for call in &calls {
                    assert_eq!(
                        history
                            .iter()
                            .filter(|message| matches!(message,
                                ChatMessage::ToolOutput(recorded, _) if recorded.id() == call.id()
                            ))
                            .count(),
                        1
                    );
                }
            }
        }
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
