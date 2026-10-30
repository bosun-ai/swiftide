#![cfg(feature = "context-management")]
use async_trait::async_trait;
use std::{
    borrow::Cow,
    collections::VecDeque,
    path::Path,
    sync::{Arc, Mutex},
};
use swiftide_agents::{Agent, DefaultContext};
use swiftide_agents_contrib::hooks::{ContextCompression, ToolSummarizer};
use swiftide_core::{
    AgentContext, ChatCompletion, Command, CommandError, CommandOutput, SimplePrompt, ToolExecutor,
    chat_completion::{
        ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ChatMessageContentPart, Tool,
        ToolCall, ToolOutput, ToolSpec,
        errors::{LanguageModelError, ToolError},
    },
    indexing::IndexingStream,
    prompt::Prompt,
};

#[derive(Clone, Debug)]
enum Reply {
    Text(&'static str),
    Empty,
    Fail,
    Tool(&'static str),
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
    fn respond(&self) -> Result<ChatCompletionResponse, LanguageModelError> {
        match self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected model call")
        {
            Reply::Text(text) => Ok(ChatCompletionResponse {
                message: Some(text.into()),
                ..Default::default()
            }),
            Reply::Empty => Ok(ChatCompletionResponse::default()),
            Reply::Tool(name) => Ok(ChatCompletionResponse {
                tool_calls: Some(vec![call(name)]),
                ..Default::default()
            }),
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
        self.respond()
    }
}
#[async_trait]
impl SimplePrompt for Model {
    async fn prompt(&self, prompt: Prompt) -> Result<String, LanguageModelError> {
        self.prompts.lock().unwrap().push(prompt.render().unwrap());
        Ok(self.respond()?.message.unwrap_or_default())
    }
}
#[derive(Clone, Debug)]
struct NoopExecutor;
#[async_trait]
impl ToolExecutor for NoopExecutor {
    async fn exec_cmd(&self, _command: &Command) -> Result<CommandOutput, CommandError> {
        Ok(CommandOutput::empty())
    }
    async fn stream_files(
        &self,
        _path: &Path,
        _extensions: Option<Vec<String>>,
    ) -> anyhow::Result<IndexingStream<String>> {
        Ok(IndexingStream::empty())
    }
}
#[derive(Clone, Debug)]
struct Echo;
#[async_trait]
impl Tool for Echo {
    async fn invoke(
        &self,
        _context: &dyn AgentContext,
        _call: &ToolCall,
    ) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::text("result"))
    }
    fn name(&self) -> Cow<'_, str> {
        "echo".into()
    }
    fn tool_spec(&self) -> ToolSpec {
        ToolSpec::builder()
            .name("echo")
            .description("A test tool")
            .build()
            .unwrap()
    }
}
fn agent() -> Agent {
    Agent::builder()
        .llm(&Model::new([]))
        .no_system_prompt()
        .add_tool(Echo)
        .context(DefaultContext::from_executor(
            Arc::new(NoopExecutor) as Arc<dyn ToolExecutor>
        ))
        .build()
        .unwrap()
}
fn call(name: &str) -> ToolCall {
    ToolCall::builder()
        .id("call")
        .name(name)
        .args("{}")
        .build()
        .unwrap()
}
fn output_builder(model: &Model) -> swiftide_agents_contrib::hooks::ToolSummarizerBuilder {
    let mut builder = ToolSummarizer::builder();
    builder
        .llm(Arc::new(model.clone()) as Arc<dyn SimplePrompt>)
        .tools_to_summarize(Vec::<&'static str>::new())
        .git_start_sha("HEAD");
    builder
}
fn conversation_builder(
    model: &Model,
) -> swiftide_agents_contrib::hooks::ContextCompressionBuilder {
    let mut builder = ContextCompression::builder();
    builder
        .llm(Arc::new(model.clone()) as Arc<dyn ChatCompletion>)
        .git_start_sha("HEAD")
        .git_diff_dir(".");
    builder
}
#[tokio::test]
async fn default_output_policy_excludes_file_reads_and_does_not_enable_automatic_summaries() {
    let model = Model::new([]);
    let hook = output_builder(&model)
        .summarize_all_at_token_cutoff(1)
        .truncate_tokens_cutoff(1)
        .build()
        .unwrap()
        .summarize_hook();
    let mut agent = agent();
    let text = "file contents".repeat(100);
    for name in ["read_file", "read_file_with_line_numbers", "Read"] {
        let mut output = Ok(ToolOutput::text(&text));
        hook(&mut agent, &call(name), &mut output).await.unwrap();
        assert_eq!(output.unwrap().as_text(), Some(text.as_str()));
    }
    let hook = output_builder(&model).build().unwrap().summarize_hook();
    let mut output = Ok(ToolOutput::fail("details".repeat(20_000)));
    hook(&mut agent, &call("echo"), &mut output).await.unwrap();
    assert!(
        output
            .unwrap()
            .as_fail()
            .unwrap()
            .contains("<content trimmed>")
    );
    assert!(model.prompts.lock().unwrap().is_empty());
}
#[tokio::test]
async fn included_tools_keep_source_prompt_and_failure_status_and_propagate_model_errors() {
    let model = Model::new([Reply::Text("short failure"), Reply::Empty, Reply::Fail]);
    let hook = output_builder(&model)
        .tools_to_summarize(vec!["echo"])
        .build()
        .unwrap()
        .summarize_hook();
    let mut agent = agent();
    let mut output = Ok(ToolOutput::fail("details"));
    hook(&mut agent, &call("echo"), &mut output).await.unwrap();
    assert_eq!(output.as_ref().unwrap().as_fail(), Some("short failure"));
    assert!(model.prompts.lock().unwrap()[0].contains("It is your job to refine the output."));
    hook(&mut agent, &call("echo"), &mut output).await.unwrap();
    assert_eq!(output.as_ref().unwrap().as_fail(), Some(""));
    assert!(hook(&mut agent, &call("echo"), &mut output).await.is_err());
}
#[tokio::test]
async fn turn_trigger_checks_the_counter_before_incrementing_and_keeps_original_prompt_and_messages()
 {
    let model = Model::new([Reply::Text("summary")]);
    let hook = conversation_builder(&model)
        .build()
        .unwrap()
        .summarize_hook();
    let mut agent = agent();
    agent
        .context()
        .add_message(ChatMessage::new_user("goal"))
        .await
        .unwrap();
    for _ in 0..5 {
        hook(&mut agent).await.unwrap();
    }
    assert!(model.requests.lock().unwrap().is_empty());
    hook(&mut agent).await.unwrap();
    {
        let requests = model.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(
            requests[0]
                .last()
                .unwrap()
                .to_string()
                .contains("Always instruct the agent to continue and finish the task.")
        );
    }
    let history = agent.context().history().await.unwrap();
    assert!(matches!(
        history.as_slice(),
        [
            ChatMessage::User(_),
            ChatMessage::Summary(_),
            ChatMessage::User(_)
        ]
    ));
    let recovery = agent
        .tools()
        .iter()
        .find(|tool| tool.name() == "read_original_instructions")
        .unwrap();
    assert_eq!(
        recovery
            .invoke(agent.context(), &call("read_original_instructions"))
            .await
            .unwrap()
            .as_text(),
        Some("goal")
    );
}
#[tokio::test]
async fn file_reads_postpone_the_source_turn_trigger_until_the_token_guard_requires_a_summary() {
    let model = Model::new([Reply::Text("summary")]);
    let hook = conversation_builder(&model)
        .num_completions_for_summary(1usize)
        .build()
        .unwrap()
        .summarize_hook();
    let mut agent = agent();
    agent
        .context()
        .add_messages(vec![
            ChatMessage::new_user("goal"),
            ChatMessage::new_tool_output(call("Read"), ToolOutput::text("file")),
        ])
        .await
        .unwrap();
    for _ in 0..3 {
        hook(&mut agent).await.unwrap();
    }
    assert!(model.requests.lock().unwrap().is_empty());
    agent
        .context()
        .add_message(ChatMessage::new_tool_output(
            call("echo"),
            ToolOutput::text("done"),
        ))
        .await
        .unwrap();
    hook(&mut agent).await.unwrap();
    assert_eq!(model.requests.lock().unwrap().len(), 1);
    assert!(
        model.requests.lock().unwrap()[0]
            .iter()
            .any(|message| message.to_string().contains("I ran a tool called: echo"))
    );
}
#[tokio::test]
async fn source_model_failure_and_empty_summary_reset_the_counter_before_the_attempt() {
    let model = Model::new([Reply::Fail, Reply::Empty]);
    let hook = conversation_builder(&model)
        .num_completions_for_summary(1usize)
        .build()
        .unwrap()
        .summarize_hook();
    let mut agent = agent();
    agent
        .context()
        .add_message(ChatMessage::new_user("goal"))
        .await
        .unwrap();
    hook(&mut agent).await.unwrap();
    assert!(hook(&mut agent).await.is_err());
    hook(&mut agent).await.unwrap();
    assert_eq!(model.requests.lock().unwrap().len(), 1);
    hook(&mut agent).await.unwrap();
    assert_eq!(model.requests.lock().unwrap().len(), 2);
    assert!(matches!(
        agent.context().history().await.unwrap().as_slice(),
        [ChatMessage::User(_)]
    ));
}
#[tokio::test]
async fn original_instruction_recovery_keeps_the_first_plain_text_message() {
    let model = Model::new([Reply::Text("summary")]);
    let hook = conversation_builder(&model)
        .num_completions_for_summary(1usize)
        .build()
        .unwrap()
        .summarize_hook();
    let mut agent = agent();
    agent
        .context()
        .add_messages(vec![
            ChatMessage::new_user_with_parts(vec![ChatMessageContentPart::text("media goal")]),
            ChatMessage::new_user("plain goal"),
        ])
        .await
        .unwrap();
    hook(&mut agent).await.unwrap();
    hook(&mut agent).await.unwrap();
    let recovery = agent
        .tools()
        .iter()
        .find(|tool| tool.name() == "read_original_instructions")
        .unwrap();
    let history = agent.context().history().await.unwrap();
    assert_eq!(
        recovery
            .invoke(agent.context(), &call("read_original_instructions"))
            .await
            .unwrap()
            .as_text(),
        Some("plain goal")
    );
    assert_eq!(agent.context().history().await.unwrap(), history);
}

#[tokio::test]
async fn final_reply_compression_keeps_the_answer_last_without_another_completion() {
    let primary = Model::new([
        Reply::Text("final answer"),
        Reply::Text("unexpected extra answer"),
    ]);
    let summaries = Model::new([Reply::Text("summary")]);
    let compression = conversation_builder(&summaries)
        .context_limit(1usize)
        .build()
        .unwrap();
    let mut builder = Agent::builder();
    builder
        .llm(&primary)
        .no_system_prompt()
        .limit(2usize)
        .context(DefaultContext::from_executor(
            Arc::new(NoopExecutor) as Arc<dyn ToolExecutor>
        ));
    compression.register_hooks(&mut builder);
    let mut agent = builder.build().unwrap();
    agent.query("goal").await.unwrap();
    assert_eq!(primary.requests.lock().unwrap().len(), 1);
    assert!(summaries.requests.lock().unwrap()[0].iter().any(
        |message| matches!(message, ChatMessage::Assistant(Some(text), _) if text == "final answer")
    ));
    assert!(matches!(agent.context().history().await.unwrap().last(),
        Some(ChatMessage::Assistant(Some(text), _)) if text == "final answer"));
}

fn running_agent(primary: &Model, summaries: &Model, context_limit: usize, endless: bool) -> Agent {
    let mut context =
        DefaultContext::from_executor(Arc::new(NoopExecutor) as Arc<dyn ToolExecutor>);
    context.with_stop_on_assistant(!endless);
    let mut builder = Agent::builder();
    builder
        .llm(primary)
        .no_system_prompt()
        .limit(3usize)
        .add_tool(Echo)
        .context(context);
    conversation_builder(summaries)
        .context_limit(context_limit)
        .build()
        .unwrap()
        .register_hooks(&mut builder);
    builder.build().unwrap()
}

#[tokio::test]
async fn compression_preserves_empty_tool_stop_and_error_lifecycles() {
    let cases = [
        ("empty reply", vec![Reply::Empty], 1, 1, false),
        (
            "tool then final reply",
            vec![Reply::Tool("echo"), Reply::Text("done")],
            2,
            2,
            false,
        ),
        (
            "tool then empty reply",
            vec![Reply::Tool("echo"), Reply::Empty],
            2,
            2,
            false,
        ),
        ("explicit stop", vec![Reply::Tool("stop")], 1, 0, false),
        ("model error", vec![Reply::Fail], 1, 0, true),
    ];
    for (name, replies, expected_calls, expected_summaries, fails) in cases {
        let primary = Model::new(replies);
        let summaries = Model::new([Reply::Text("summary"), Reply::Text("summary")]);
        let mut agent = running_agent(&primary, &summaries, 1, false);
        assert_eq!(agent.query("goal").await.is_err(), fails, "{name}");
        assert!(agent.is_stopped(), "{name}");
        assert_eq!(
            primary.requests.lock().unwrap().len(),
            expected_calls,
            "{name}"
        );
        assert_eq!(
            summaries.requests.lock().unwrap().len(),
            expected_summaries,
            "{name}"
        );
    }
}

#[tokio::test]
async fn empty_summary_does_not_create_another_request() {
    for reply in [Reply::Empty, Reply::Text("done")] {
        let primary = Model::new([reply]);
        let summaries = Model::new([Reply::Empty]);
        let mut agent = running_agent(&primary, &summaries, 1, false);
        agent.query("goal").await.unwrap();
        assert_eq!(primary.requests.lock().unwrap().len(), 1);
        assert_eq!(summaries.requests.lock().unwrap().len(), 1);
        assert!(
            !agent
                .context()
                .history()
                .await
                .unwrap()
                .iter()
                .any(ChatMessage::is_summary)
        );
    }
}

#[tokio::test]
async fn persistent_and_single_step_queries_keep_final_answers_last() {
    for once in [false, true] {
        let primary = Model::new([Reply::Text("first answer"), Reply::Text("second answer")]);
        let summaries = Model::new([Reply::Text("summary"), Reply::Text("summary")]);
        let mut agent = running_agent(&primary, &summaries, 1, false);
        for query in ["first goal", "second goal"] {
            if once {
                agent.query_once(query).await.unwrap();
            } else {
                agent.query(query).await.unwrap();
            }
        }
        assert_eq!(primary.requests.lock().unwrap().len(), 2);
        assert_eq!(summaries.requests.lock().unwrap().len(), 2);
        assert!(matches!(agent.context().history().await.unwrap().last(),
            Some(ChatMessage::Assistant(Some(text), _)) if text == "second answer"));
    }
}

#[tokio::test]
async fn endless_text_turns_compress_before_later_requests() {
    for context_limit in [1, usize::MAX] {
        let primary = Model::new([
            Reply::Text("working"),
            Reply::Text("working"),
            Reply::Text("working"),
        ]);
        let summaries = Model::new([
            Reply::Text("summary"),
            Reply::Text("summary"),
            Reply::Text("summary"),
        ]);
        let mut context =
            DefaultContext::from_executor(Arc::new(NoopExecutor) as Arc<dyn ToolExecutor>);
        context.with_stop_on_assistant(false);
        let mut builder = Agent::builder();
        builder
            .llm(&primary)
            .no_system_prompt()
            .limit(3usize)
            .context(context);
        conversation_builder(&summaries)
            .num_completions_for_summary(1usize)
            .context_limit(context_limit)
            .build()
            .unwrap()
            .register_hooks(&mut builder);
        builder.build().unwrap().query("goal").await.unwrap();
        let requests = primary.requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert!(requests.last().unwrap().iter().any(ChatMessage::is_summary));
    }
}

#[tokio::test]
async fn empty_reply_does_not_count_the_previous_tool_output_again() {
    let primary = Model::new([Reply::Tool("echo"), Reply::Empty]);
    let summaries = Model::new([]);
    let mut builder = Agent::builder();
    builder
        .llm(&primary)
        .no_system_prompt()
        .add_tool(Echo)
        .limit(3usize)
        .context(DefaultContext::from_executor(
            Arc::new(NoopExecutor) as Arc<dyn ToolExecutor>
        ));
    conversation_builder(&summaries)
        .num_completions_for_summary(2usize)
        .build()
        .unwrap()
        .register_hooks(&mut builder);
    builder.build().unwrap().query("goal").await.unwrap();
    assert_eq!(primary.requests.lock().unwrap().len(), 2);
    assert!(summaries.requests.lock().unwrap().is_empty());
}
