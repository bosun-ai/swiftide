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
