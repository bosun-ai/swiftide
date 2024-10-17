use anyhow::Result;
use async_trait::async_trait;
use derive_builder::Builder;
use dyn_clone::DynClone;
use swiftide_core::prompt::Prompt;

use crate::agent::Agent;

#[async_trait]
pub trait Workspace: Send + Sync + DynClone {
    // tbd naming
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput>;

    // Ensures commands can be run
    // tbd what to do with git setup etc

    // Maybe leave it to user?
    async fn init(&self) -> Result<()>;

    async fn teardown(self);
}

#[async_trait]
pub trait Tool: Send + Sync + DynClone {
    // tbd
    async fn invoke(
        &self,
        agent_context: &dyn AgentContext,
        raw_args: Option<&str>,
    ) -> Result<ToolOutput>;

    fn name(&self) -> &'static str;

    // Ideas:
    // Typed instead of string
    // LLMs have different requirements, validators?
    fn json_spec(&self) -> JsonSpec;
}

#[async_trait]
pub trait ChatCompletion: Send + Sync + DynClone {
    async fn complete(&self, request: ChatCompletionRequest) -> Result<ChatCompletionResponse>;
}

/// Acts as the interface to the external world and any overlapping state
/// NOTE: Async as expecting locks
#[async_trait]
pub trait AgentContext: Send + Sync + DynClone {
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput>;

    /// List of all messages for this agent, for the purpose of completion and logs
    async fn message_history(&self) -> &[ChatMessage];

    /// Receives a message and adds it to the history
    async fn received_message(&mut self, message: &ChatMessage);

    async fn received_tool_call(&mut self, message: &ToolCall);
}

// TMP
pub enum Command {
    Shell(String),
    // Git, Github, File, Code, etc
}

pub enum CommandOutput {
    // tbd
    Stdout(String),
    Stderr(String),
    Status(i32),
}

// Idea to have semantically usuable types that have default behaviour
// Additionally, unlike Fluyt, handle as much as possible via tools
//
// Maybe this should be a struct?
pub enum ToolOutput {
    /// Adds the result of the toolcall to messages
    ToolCall(ToolCall),
    /// Stops an agent
    Stop(Success),
    //Raw(String),
    //Agent(Agent),
}

/// TODO: Needs more values, i.e. OpenAI needs a reference to the original call
pub type ToolCall = String;

type Success = bool;

pub struct ChatCompletionResponse {
    pub message: String,

    // Can be a better type
    // Perhaps should be typed to actual functions already?
    pub tool_invocations: Vec<(String, Option<String>)>,
}

#[derive(Builder, Clone)]
#[builder(setter(into, strip_option))]
pub struct ChatCompletionRequest<'a> {
    system_prompt: Option<&'a Prompt>,
    messages: &'a [ChatMessage],
    tools_spec: Vec<JsonSpec>,
}

impl<'a> ChatCompletionRequest<'a> {
    pub fn builder() -> ChatCompletionRequestBuilder<'a> {
        ChatCompletionRequestBuilder::default()
    }
}

type ChatMessage = String;
type JsonSpec = &'static str;
