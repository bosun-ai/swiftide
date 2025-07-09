use std::{collections::HashMap, sync::Arc};

use futures_util::lock::Mutex;
use serde_json::json;
use swiftide_agents::{
    Agent, StopReason, chat_request,
    tasks::{
        self, Task,
        tools::{default_complete_toolspec, default_delegate_toolspec},
    },
    user,
};
use swiftide_core::{
    chat_completion::{ChatCompletionResponse, ToolCall},
    test_utils::MockChatCompletion,
};

// Things to test:
// - No actions
// - Delegate with and back
// - Delegate without and back
// - Task completed
#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn test_simple_delegate_task() {
    // Completions agent1 will do
    let agent1_llm = MockChatCompletion::new();

    let expected_request = chat_request! {
        user!("Do a task thing");

        tool_specs = [default_delegate_toolspec("delegate_agent_1")]
    };

    let response = ChatCompletionResponse::builder()
        .tool_calls(vec![
            ToolCall::builder()
                .id("1")
                .name("delegate_agent_1")
                .args(
                    json!({
                        "instructions": "Hello agent2"

                    })
                    .to_string(),
                )
                .build()
                .unwrap(),
        ])
        .build()
        .unwrap();

    agent1_llm.expect_complete(expected_request, Ok(response));

    // Completions agent2 will do
    let agent2_llm = MockChatCompletion::new();

    let expected_request = chat_request! {
        user!("Hello agent2");

        tool_specs = [default_complete_toolspec("complete_task")]
    };

    let response = ChatCompletionResponse::builder()
        .tool_calls(vec![
            ToolCall::builder()
                .id("1")
                .name("complete_task")
                .build()
                .unwrap(),
        ])
        .build()
        .unwrap();

    agent2_llm.expect_complete(expected_request, Ok(response));

    // Quick double check on builder generics
    let _builder = Task::builder().state(Arc::new(Mutex::new(HashMap::<String, String>::new())));

    // Now we run the task and see if it works
    let task = Task::builder()
        .agents([
            Agent::builder()
                .name("agent1")
                .llm(&agent1_llm)
                .no_system_prompt()
                .build()
                .unwrap(),
            Agent::builder()
                .name("agent2")
                .llm(&agent2_llm)
                .no_system_prompt()
                .build()
                .unwrap(),
        ])
        .with(tasks::Action::for_agent("agent1").delegates_to("agent2"))
        .with(tasks::Action::for_agent("agent2").can_complete())
        .starts_with("agent1")
        .build()
        .await
        .unwrap();

    task.query("Do a task thing").await.unwrap();

    task.join_all().await.unwrap();
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn test_abort() {
    let agent1_llm = MockChatCompletion::new();

    let expected_request = chat_request! {
        user!("Do a task thing");

        tool_specs = [default_delegate_toolspec("delegate_agent_1")]
    };

    let response = ChatCompletionResponse::builder()
        .tool_calls(vec![
            ToolCall::builder()
                .id("1")
                .name("delegate_agent_1")
                .args(
                    json!({
                        "instructions": "Hello agent2"

                    })
                    .to_string(),
                )
                .build()
                .unwrap(),
        ])
        .build()
        .unwrap();

    agent1_llm.expect_complete(expected_request, Ok(response));

    // Completions agent2 will do
    let agent2_llm = MockChatCompletion::new();

    let expected_request = chat_request! {
        user!("Hello agent2");

        tool_specs = [default_complete_toolspec("complete_task")]
    };

    let response = ChatCompletionResponse::builder().build().unwrap();

    agent2_llm.expect_complete(expected_request, Ok(response));

    // Quick double check on builder generics
    let _builder = Task::builder().state(Arc::new(Mutex::new(HashMap::<String, String>::new())));

    // Now we run the task and see if it works
    let mut task = Task::builder()
        .agents([
            Agent::builder()
                .name("agent1")
                .llm(&agent1_llm)
                .no_system_prompt()
                .build()
                .unwrap(),
            Agent::builder()
                .name("agent2")
                .llm(&agent2_llm)
                .no_system_prompt()
                .build()
                .unwrap(),
        ])
        .with(tasks::Action::for_agent("agent1").delegates_to("agent2"))
        .with(tasks::Action::for_agent("agent2").can_complete())
        .starts_with("agent1")
        .build()
        .await
        .unwrap();

    task.query("Do a task thing").await.unwrap();

    task.abort().await;
    task.join_all().await.unwrap();

    assert_eq!(task.outstanding(), 0);
    assert_eq!(
        task.current_agent()
            .await
            .unwrap()
            .lock()
            .await
            .stop_reason(),
        Some(&StopReason::TaskAborted)
    );
}
