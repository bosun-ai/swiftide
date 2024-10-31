use std::collections::HashSet;

use anyhow::Result;
use async_trait::async_trait;
use derive_builder::Builder;
use dyn_clone::DynClone;

#[async_trait]
pub trait ChatCompletion: Send + Sync + DynClone {
    async fn complete(&self, request: &ChatCompletionRequest) -> Result<ChatCompletionResponse>;
}

#[async_trait]
impl ChatCompletion for Box<dyn ChatCompletion> {
    async fn complete(&self, request: &ChatCompletionRequest) -> Result<ChatCompletionResponse> {
        (**self).complete(request).await
    }
}

#[async_trait]
impl ChatCompletion for &dyn ChatCompletion {
    async fn complete(&self, request: &ChatCompletionRequest) -> Result<ChatCompletionResponse> {
        (**self).complete(request).await
    }
}

#[async_trait]
impl<T> ChatCompletion for &T
where
    T: ChatCompletion + Clone + 'static,
{
    async fn complete(&self, request: &ChatCompletionRequest) -> Result<ChatCompletionResponse> {
        (**self).complete(request).await
    }
}

impl<LLM> From<&LLM> for Box<dyn ChatCompletion>
where
    LLM: ChatCompletion + Clone + 'static,
{
    fn from(llm: &LLM) -> Self {
        Box::new(llm.clone())
    }
}

dyn_clone::clone_trait_object!(ChatCompletion);

#[derive(Clone, Builder, Debug)]
#[builder(setter(strip_option, into), build_fn(error = anyhow::Error))]
pub struct ChatCompletionResponse {
    pub message: Option<String>,

    // Can be a better type
    // Perhaps should be typed to actual functions already?
    #[builder(default)]
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl ChatCompletionResponse {
    pub fn builder() -> ChatCompletionResponseBuilder {
        ChatCompletionResponseBuilder::default()
    }
}

impl ChatCompletionResponseBuilder {
    pub fn maybe_message<T: Into<Option<String>>>(&mut self, message: T) -> &mut Self {
        self.message = Some(message.into());
        self
    }

    pub fn maybe_tool_calls<T: Into<Option<Vec<ToolCall>>>>(&mut self, tool_calls: T) -> &mut Self {
        self.tool_calls = Some(tool_calls.into());
        self
    }
}

#[derive(Builder, Clone, PartialEq, Debug)]
#[builder(setter(into, strip_option))]
pub struct ChatCompletionRequest {
    // TODO: Alternatively maybe, we can also have an instruction, and build a system prompt for it
    // and add it to message if present
    messages: Vec<ChatMessage>,
    #[builder(default)]
    tools_spec: HashSet<JsonSpec>,
}

impl ChatCompletionRequest {
    pub fn builder() -> ChatCompletionRequestBuilder {
        ChatCompletionRequestBuilder::default()
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }
}

#[derive(Clone, strum_macros::EnumIs, PartialEq, Debug)]
pub enum ChatMessage {
    System(String),
    User(String),
    Assistant(String),
    ToolCall(ToolCall),
    ToolOutput(ToolCall, ToolOutput),
}

pub enum ChatRole {
    System,
    User,
    Assistant,
}

// TODO: Naming
#[derive(Debug, Clone, PartialEq)]
pub enum ToolOutput {
    /// Adds the result of the toolcall to messages
    Content(String),
    /// Stops an agent
    ///
    Stop,
}

impl ToolOutput {
    pub fn content(&self) -> Option<&str> {
        match self {
            ToolOutput::Content(s) => Some(s),
            _ => None,
        }
    }
}

impl<T: AsRef<str>> From<T> for ToolOutput {
    fn from(s: T) -> Self {
        ToolOutput::Content(s.as_ref().to_string())
    }
}

/// TODO: Needs more values, i.e. `OpenAI` needs a reference to the original call
#[derive(Clone, Debug, Builder, PartialEq)]
#[builder(setter(into, strip_option))]
pub struct ToolCall {
    id: String,
    name: String,
    #[builder(default)]
    args: Option<String>,
}

impl ToolCall {
    pub fn builder() -> ToolCallBuilder {
        ToolCallBuilder::default()
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn args(&self) -> Option<&str> {
        self.args.as_deref()
    }
}

pub type JsonSpec = &'static str;
