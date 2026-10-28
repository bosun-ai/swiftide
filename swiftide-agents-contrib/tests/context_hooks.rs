#![cfg(feature = "context-management")]

use async_trait::async_trait;
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use swiftide_agents::{
    Agent,
    hooks::{AfterEachFn, Hook},
};
use swiftide_agents_contrib::hooks::{ContextCompression, ToolSummarizer};
use swiftide_core::{
    ChatCompletion, SimplePrompt,
    chat_completion::{
        ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ChatMessageContentPart,
        ToolCall, ToolOutput,
        errors::{LanguageModelError, ToolError},
    },
    prompt::Prompt,
    token_estimation::{Estimatable, EstimateTokens},
};

#[derive(Clone, Debug)]
enum Reply {
    Text(&'static str),
    Tools(Vec<ToolCall>),
    Empty,
    Fail,
}

#[derive(Clone, Debug)]
struct Model {
    replies: Arc<Mutex<VecDeque<Reply>>>,
    requests: Arc<Mutex<Vec<Vec<ChatMessage>>>>,
    prompts: Arc<Mutex<Vec<String>>>,
}

impl Model {
    fn new(replies: impl IntoIterator<Item = Reply>) -> Self {
        Self {
            replies: Arc::new(Mutex::new(replies.into_iter().collect())),
            requests: Arc::default(),
            prompts: Arc::default(),
        }
    }
    fn respond(&self) -> Result<Option<String>, LanguageModelError> {
        match self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected model call")
        {
            Reply::Tools(_) => panic!("tool calls require ChatCompletion"),
            Reply::Text(text) => Ok(Some(text.into())),
            Reply::Empty => Ok(None),
            Reply::Fail => Err(LanguageModelError::permanent(std::io::Error::other(
                "unavailable",
            ))),
        }
    }
}

#[async_trait]
impl ChatCompletion for Model {
    async fn complete(
        &self,
        request: &ChatCompletionRequest<'_>,
    ) -> Result<ChatCompletionResponse, LanguageModelError> {
        self.requests
            .lock()
            .unwrap()
            .push(request.messages().to_vec());
        let calls = {
            let mut replies = self.replies.lock().unwrap();
            if matches!(replies.front(), Some(Reply::Tools(_))) {
                match replies.pop_front().unwrap() {
                    Reply::Tools(calls) => Some(calls),
                    _ => unreachable!(),
                }
            } else {
                None
            }
        };
        if let Some(tool_calls) = calls {
            return Ok(ChatCompletionResponse {
                tool_calls: Some(tool_calls),
                ..Default::default()
            });
        }
        Ok(ChatCompletionResponse {
            message: self.respond()?,
            ..Default::default()
        })
    }
}

#[async_trait]
impl SimplePrompt for Model {
    async fn prompt(&self, prompt: Prompt) -> Result<String, LanguageModelError> {
        self.prompts.lock().unwrap().push(prompt.render().unwrap());
        Ok(self.respond()?.unwrap_or_default())
    }
}

fn agent() -> Agent {
    Agent::builder()
        .llm(&Model::new([]))
        .no_system_prompt()
        .build()
        .unwrap()
}

fn compression_hook<E: EstimateTokens + Send + Sync + 'static>(
    compression: ContextCompression<E>,
) -> Box<dyn AfterEachFn> {
    let Some(Hook::AfterEach(hook)) = compression.hooks().pop() else {
        unreachable!()
    };
    hook
}

fn call(name: &str) -> ToolCall {
    ToolCall::builder()
        .id("result")
        .name(name)
        .args("{}")
        .build()
        .unwrap()
}

#[tokio::test]
async fn truncation_needs_no_model_and_preserves_failure_variants() {
    let hook = ToolSummarizer::builder()
        .max_output_chars(80)
        .build()
        .unwrap()
        .summarize_hook();
    let mut agent = agent();
    let mut output = Ok(ToolOutput::fail("α".repeat(200)));
    hook(&mut agent, &call("tests"), &mut output).await.unwrap();
    let output = output.unwrap();
    assert!(output.as_fail().is_some());
    assert!(output.content().unwrap().chars().count() <= 80);
    assert!(output.content().unwrap().contains("<content trimmed>"));
}

#[tokio::test]
async fn exclusions_and_control_outputs_bypass_all_rewriting() {
    let model = Model::new([]);
    let hook = ToolSummarizer::builder()
        .llm(Arc::new(model.clone()))
        .tools_to_summarize(["Read"])
        .excluded_tools(["Read"])
        .max_output_chars(20)
        .build()
        .unwrap()
        .summarize_hook();
    let mut agent = agent();
    let text = "contents".repeat(100);
    let mut output = Ok(ToolOutput::text(&text));
    hook(&mut agent, &call("Read"), &mut output).await.unwrap();
    assert_eq!(output.unwrap().as_text(), Some(text.as_str()));
    for control in [
        ToolOutput::stop(),
        ToolOutput::feedback_required(None),
        ToolOutput::AgentFailed(None),
    ] {
        let mut output = Ok(control.clone());
        hook(&mut agent, &call("other"), &mut output).await.unwrap();
        assert_eq!(output.unwrap(), control);
    }
    let mut output = Err(ToolError::unknown(std::io::Error::other("executor failed")));
    hook(&mut agent, &call("other"), &mut output).await.unwrap();
    assert!(output.is_err());
    assert!(model.prompts.lock().unwrap().is_empty());
}

#[tokio::test]
async fn summaries_preserve_failure_status_and_model_failures_keep_usable_output() {
    let model = Model::new([Reply::Text("concise error"), Reply::Fail, Reply::Empty]);
    let hook = ToolSummarizer::builder()
        .llm(Arc::new(model.clone()))
        .tools_to_summarize(["tests"])
        .max_output_chars(80)
        .build()
        .unwrap()
        .summarize_hook();
    let mut agent = agent();
    let mut output = Ok(ToolOutput::fail("errors".repeat(100)));
    hook(&mut agent, &call("tests"), &mut output).await.unwrap();
    assert_eq!(output.unwrap().as_fail(), Some("concise error"));
    for _ in 0..2 {
        let mut output = Ok(ToolOutput::text("details".repeat(100)));
        hook(&mut agent, &call("tests"), &mut output).await.unwrap();
        let output = output.unwrap();
        assert!(output.as_text().unwrap().contains("<content trimmed>"));
    }
    assert_eq!(model.prompts.lock().unwrap().len(), 3);
}

struct TokenCount(Arc<AtomicUsize>);

#[tokio::test]
async fn tool_summary_threshold_uses_the_custom_estimator_and_prompt_context() {
    let tokens = Arc::new(AtomicUsize::new(10));
    let model = Model::new([Reply::Text("short")]);
    let hook = ToolSummarizer::builder()
        .llm(Arc::new(model.clone()))
        .summarize_at_tokens(10)
        .token_estimator(TokenCount(Arc::clone(&tokens)))
        .prompt(
            Prompt::from("{{extra}} {{tool_name}}: {{tool_output}}")
                .with_context_value("extra", "custom"),
        )
        .build()
        .unwrap()
        .summarize_hook();
    let mut agent = agent();
    let mut output = Ok(ToolOutput::text("details"));
    hook(&mut agent, &call("tests"), &mut output).await.unwrap();
    assert!(model.prompts.lock().unwrap().is_empty());
    tokens.store(11, Ordering::SeqCst);
    hook(&mut agent, &call("tests"), &mut output).await.unwrap();
    assert_eq!(output.unwrap().as_text(), Some("short"));
    assert_eq!(model.prompts.lock().unwrap()[0], "custom tests: details");
}

#[tokio::test]
async fn stopped_agents_and_empty_history_do_not_request_summaries() {
    let model = Model::new([]);
    let compression = ContextCompression::builder()
        .llm(Arc::new(model.clone()))
        .summarize_every_turns(1)
        .build()
        .unwrap();
    let hook = compression_hook(compression);
    let mut agent = agent();
    hook(&mut agent).await.unwrap();
    agent
        .context()
        .add_message(ChatMessage::new_user("goal"))
        .await
        .unwrap();
    agent.stop("finished").await;
    hook(&mut agent).await.unwrap();
    assert!(model.requests.lock().unwrap().is_empty());
}
#[async_trait]
impl EstimateTokens for TokenCount {
    async fn estimate(&self, _value: impl Estimatable) -> anyhow::Result<usize> {
        Ok(self.0.load(Ordering::SeqCst))
    }
}

#[tokio::test]
async fn token_threshold_uses_the_configured_estimator() {
    let tokens = Arc::new(AtomicUsize::new(10));
    let model = Model::new([Reply::Text("summary")]);
    let compression = ContextCompression::builder()
        .llm(Arc::new(model.clone()))
        .context_limit(10)
        .summarize_every_turns(0)
        .token_estimator(TokenCount(Arc::clone(&tokens)))
        .build()
        .unwrap();
    let hook = compression_hook(compression);
    let mut agent = agent();
    agent
        .context()
        .add_message(ChatMessage::new_user("goal"))
        .await
        .unwrap();
    hook(&mut agent).await.unwrap();
    assert!(model.requests.lock().unwrap().is_empty());
    tokens.store(11, Ordering::SeqCst);
    hook(&mut agent).await.unwrap();
    assert_eq!(model.requests.lock().unwrap().len(), 1);
    assert!(matches!(
        agent.context().history().await.unwrap().as_slice(),
        [
            ChatMessage::User(_),
            ChatMessage::Summary(_),
            ChatMessage::User(_)
        ]
    ));
}

#[tokio::test]
async fn failed_and_empty_summaries_do_not_reset_the_turn_trigger() {
    let model = Model::new([
        Reply::Fail,
        Reply::Empty,
        Reply::Text("kept"),
        Reply::Text("next"),
    ]);
    let compression = ContextCompression::builder()
        .llm(Arc::new(model.clone()))
        .summarize_every_turns(2)
        .build()
        .unwrap();
    let hook = compression_hook(compression);
    let mut agent = agent();
    agent
        .context()
        .add_message(ChatMessage::new_user("goal"))
        .await
        .unwrap();
    let original = agent.context().history().await.unwrap();
    hook(&mut agent).await.unwrap();
    assert!(model.requests.lock().unwrap().is_empty());
    hook(&mut agent).await.unwrap();
    assert_eq!(agent.context().history().await.unwrap(), original);
    hook(&mut agent).await.unwrap();
    assert_eq!(agent.context().history().await.unwrap(), original);
    hook(&mut agent).await.unwrap();
    assert_eq!(model.requests.lock().unwrap().len(), 3);
    hook(&mut agent).await.unwrap();
    assert_eq!(model.requests.lock().unwrap().len(), 3);
    hook(&mut agent).await.unwrap();
    assert_eq!(model.requests.lock().unwrap().len(), 4);
    let requests = model.requests.lock().unwrap();
    assert!(
        requests[3]
            .iter()
            .any(|message| message.to_string().contains("kept"))
    );
}

#[tokio::test]
async fn original_instructions_recover_text_and_reattach_media_after_repeated_compression() {
    for original in [
        ChatMessage::new_user("original goal"),
        ChatMessage::new_user_with_parts(vec![
            ChatMessageContentPart::text("original goal"),
            ChatMessageContentPart::image("data:image/png;base64,AA=="),
        ]),
    ] {
        let model = Model::new([Reply::Text("one"), Reply::Text("two")]);
        let compression = ContextCompression::builder()
            .llm(Arc::new(model))
            .summarize_every_turns(1)
            .build()
            .unwrap();
        let hook = compression_hook(compression);
        let mut agent = agent();
        agent.context().add_message(original.clone()).await.unwrap();
        hook(&mut agent).await.unwrap();
        hook(&mut agent).await.unwrap();
        let tool = agent
            .tools()
            .iter()
            .find(|tool| tool.name() == "read_original_instructions")
            .unwrap();
        let output = tool
            .invoke(agent.context(), &call("read_original_instructions"))
            .await
            .unwrap();
        if let ChatMessage::User(text) = &original {
            assert_eq!(output.as_text(), Some(text.as_str()));
        } else {
            assert_ne!(
                agent.context().history().await.unwrap().last(),
                Some(&original)
            );
            hook(&mut agent).await.unwrap();
            assert_eq!(
                agent.context().history().await.unwrap().last(),
                Some(&original)
            );
        }
    }
}

#[tokio::test]
async fn output_summary_and_conversation_compression_compose_without_orphan_tool_calls() {
    let model = Model::new([Reply::Text("test outcome")]);
    let summarize = ToolSummarizer::builder()
        .llm(Arc::new(model))
        .tools_to_summarize(["tests"])
        .build()
        .unwrap()
        .summarize_hook();
    let compression_model = Model::new([Reply::Text("conversation outcome")]);
    let compression = ContextCompression::builder()
        .llm(Arc::new(compression_model.clone()))
        .summarize_every_turns(1)
        .build()
        .unwrap();
    let hook = compression_hook(compression);
    let mut agent = agent();
    let call = call("tests");
    let mut output = Ok(ToolOutput::fail("long test output"));
    summarize(&mut agent, &call, &mut output).await.unwrap();
    agent
        .context()
        .add_messages(vec![
            ChatMessage::new_user("goal"),
            ChatMessage::new_assistant(None::<String>, Some(vec![call.clone()])),
            ChatMessage::new_tool_output(call, output.unwrap()),
        ])
        .await
        .unwrap();
    hook(&mut agent).await.unwrap();
    {
        let requests = compression_model.requests.lock().unwrap();
        assert!(
            requests[0]
                .iter()
                .any(|message| message.to_string().contains("test outcome"))
        );
        assert!(!requests[0].iter().any(|message| matches!(
            message,
            ChatMessage::Assistant(_, Some(_)) | ChatMessage::ToolOutput(..)
        )));
    }
    let next = agent.context().next_completion().await.unwrap().unwrap();
    assert!(matches!(
        next.as_slice(),
        [ChatMessage::Summary(_), ChatMessage::User(_)]
    ));
}

#[tokio::test]
async fn compression_does_not_restart_a_final_answer_and_keeps_the_turn_counter() {
    let summary_model = Model::new([Reply::Text("summary")]);
    let compression = ContextCompression::builder()
        .llm(Arc::new(summary_model.clone()))
        .summarize_every_turns(1)
        .build()
        .unwrap();
    let hooks = compression.hooks();
    let answering_model = Model::new([Reply::Text("finished")]);
    let mut agent = Agent::builder()
        .llm(&answering_model)
        .no_system_prompt()
        .hooks(hooks)
        .build()
        .unwrap();
    agent.query("goal").await.unwrap();
    assert_eq!(answering_model.requests.lock().unwrap().len(), 1);
    assert!(summary_model.requests.lock().unwrap().is_empty());
    assert!(matches!(
        agent.context().history().await.unwrap().last(),
        Some(ChatMessage::Assistant(_, None))
    ));
    // Final answers count toward the next trigger without forcing another completion.
    let summary_model = Model::new([Reply::Text("summary")]);
    let compression = ContextCompression::builder()
        .llm(Arc::new(summary_model.clone()))
        .summarize_every_turns(2)
        .build()
        .unwrap();
    let hook = compression_hook(compression);
    let mut next_agent = Agent::builder()
        .llm(&answering_model)
        .no_system_prompt()
        .build()
        .unwrap();
    next_agent
        .context()
        .add_messages(vec![
            ChatMessage::new_user("goal"),
            ChatMessage::new_assistant(Some("finished"), None),
        ])
        .await
        .unwrap();
    hook(&mut next_agent).await.unwrap();
    next_agent
        .context()
        .add_message(ChatMessage::new_user("follow up"))
        .await
        .unwrap();
    hook(&mut next_agent).await.unwrap();
    assert_eq!(summary_model.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn recovered_media_follows_all_parallel_tool_results() {
    let original = ChatMessage::new_user_with_parts(vec![
        ChatMessageContentPart::text("original goal"),
        ChatMessageContentPart::image("data:image/png;base64,AA=="),
    ]);
    let summary_model = Model::new([Reply::Text("summary")]);
    let compression = ContextCompression::builder()
        .llm(Arc::new(summary_model))
        .summarize_every_turns(1)
        .build()
        .unwrap();
    let hooks = compression.hooks();
    let Hook::AfterEach(hook) = hooks[1].clone() else {
        unreachable!()
    };
    let calls = (0..2)
        .map(|id| {
            ToolCall::builder()
                .id(id.to_string())
                .name("read_original_instructions")
                .args("{}")
                .build()
                .unwrap()
        })
        .collect();
    let model = Model::new([Reply::Tools(calls), Reply::Text("finished")]);
    let mut agent = Agent::builder()
        .llm(&model)
        .no_system_prompt()
        .hooks(hooks)
        .build()
        .unwrap();
    agent.context().add_message(original.clone()).await.unwrap();
    hook(&mut agent).await.unwrap();
    agent.run().await.unwrap();
    let requests = model.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(matches!(
        requests[1].as_slice(),
        [
            ChatMessage::Summary(_),
            ChatMessage::User(_),
            ChatMessage::Assistant(_, Some(_)),
            ChatMessage::ToolOutput(..),
            ChatMessage::ToolOutput(..),
            ChatMessage::UserWithParts(_),
        ]
    ));
    assert_eq!(requests[1].last(), Some(&original));
}

#[tokio::test]
async fn empty_completions_do_not_restart_user_or_tool_turns() {
    for (trigger_by_tokens, after_tools) in
        [(false, false), (false, true), (true, false), (true, true)]
    {
        let summary_model = Model::new([]);
        let mut compression = ContextCompression::builder();
        if trigger_by_tokens {
            compression = compression.summarize_every_turns(0).context_limit(1);
        } else {
            // Tool exchanges below count as one turn; the empty response reaches the threshold.
            compression = compression.summarize_every_turns(if after_tools { 2 } else { 1 });
        }
        let tokens = Arc::new(AtomicUsize::new(0));
        let hooks = compression
            .llm(Arc::new(summary_model.clone()))
            .token_estimator(TokenCount(Arc::clone(&tokens)))
            .build()
            .unwrap()
            .hooks();
        let replies = if after_tools {
            vec![Reply::Tools(vec![call("echo")]), Reply::Empty]
        } else {
            vec![Reply::Empty]
        };
        let model = Model::new(replies);
        let mut builder = Agent::builder();
        builder.llm(&model).no_system_prompt().hooks(hooks);
        let step = Arc::new(AtomicUsize::new(0));
        builder.before_completion(move |_, _| {
            let first = step.fetch_add(1, Ordering::SeqCst) == 0;
            tokens.store(if first && after_tools { 0 } else { 2 }, Ordering::SeqCst);
            Box::pin(async { Ok(()) })
        });
        builder.add_tool(Echo);
        let mut agent = builder.build().unwrap();
        agent.query("goal").await.unwrap();
        assert_eq!(
            model.requests.lock().unwrap().len(),
            if after_tools { 2 } else { 1 }
        );
        assert!(summary_model.requests.lock().unwrap().is_empty());
        let history = agent.context().history().await.unwrap();
        assert!(
            !history
                .iter()
                .any(|message| matches!(message, ChatMessage::Summary(_)))
        );
    }
}

#[derive(Clone)]
struct Echo;

#[async_trait]
impl swiftide_core::chat_completion::Tool for Echo {
    async fn invoke(
        &self,
        _context: &dyn swiftide_core::AgentContext,
        _call: &ToolCall,
    ) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::text("tool result"))
    }
    fn name(&self) -> std::borrow::Cow<'_, str> {
        "echo".into()
    }
    fn tool_spec(&self) -> swiftide_core::chat_completion::ToolSpec {
        swiftide_core::chat_completion::ToolSpec::builder()
            .name("echo")
            .description("Return a test result")
            .build()
            .unwrap()
    }
}

#[tokio::test]
async fn tool_exchanges_still_compress_with_all_lifecycle_hooks_installed() {
    let summary_model = Model::new([Reply::Text("tool outcome")]);
    let compression = ContextCompression::builder()
        .llm(Arc::new(summary_model.clone()))
        .summarize_every_turns(1)
        .build()
        .unwrap();
    let model = Model::new([Reply::Tools(vec![call("echo")]), Reply::Text("finished")]);
    let mut builder = Agent::builder();
    builder.llm(&model).no_system_prompt().add_tool(Echo);
    let tool_hook_calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&tool_hook_calls);
    builder.after_tool(move |_, _, _| {
        observed.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    });
    for hook in compression.hooks() {
        builder.add_hook(hook);
    }
    let mut agent = builder.build().unwrap();
    agent.query("goal").await.unwrap();
    assert_eq!(tool_hook_calls.load(Ordering::SeqCst), 1);
    assert_eq!(summary_model.requests.lock().unwrap().len(), 1);
    let requests = model.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(matches!(
        requests[1].as_slice(),
        [ChatMessage::Summary(_), ChatMessage::User(_)]
    ));
}
