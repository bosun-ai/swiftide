use anyhow::Result;
use async_trait::async_trait;
use derive_builder::Builder;
use dyn_clone::DynClone;

use crate::prompt::Prompt;

#[async_trait]
pub trait ChatCompletion: Send + Sync + DynClone {
    async fn complete(&self, request: &ChatCompletionRequest) -> Result<ChatCompletionResponse>;
}

#[derive(Clone, Builder)]
#[builder(build_fn(error = anyhow::Error))]
pub struct ChatCompletionResponse {
    pub message: Option<String>,

    // Can be a better type
    // Perhaps should be typed to actual functions already?
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl ChatCompletionResponse {
    pub fn builder() -> ChatCompletionResponseBuilder {
        ChatCompletionResponseBuilder::default()
    }
}

#[derive(Builder, Clone)]
#[builder(setter(into, strip_option))]
pub struct ChatCompletionRequest<'a> {
    // TODO: Alternatively maybe, we can also have an instruction, and build a system prompt for it
    // and add it to message if present
    system_prompt: Option<&'a Prompt>,
    messages: &'a [ChatMessage],
    tools_spec: Vec<JsonSpec>,
}

impl<'a> ChatCompletionRequest<'a> {
    pub fn builder() -> ChatCompletionRequestBuilder<'a> {
        ChatCompletionRequestBuilder::default()
    }

    pub fn messages(&self) -> &[ChatMessage] {
        self.messages
    }
}

#[derive(Clone, strum_macros::EnumIs)]
pub enum ChatMessage {
    System(String),
    User(String),
    Assistant(String),
    ToolCall(ToolCall),
    ToolOutput(ToolOutput),
}

pub enum ChatRole {
    System,
    User,
    Assistant,
}

#[derive(Clone)]
pub enum ToolOutput {
    /// Adds the result of the toolcall to messages
    Content {
        tool_call: ToolCall,
        content: String,
    },
    /// Stops an agent
    ///
    Stop(bool),
    //Raw(String),
    //Agent(Agent),
}

impl ToolOutput {
    pub fn tool_call(&self) -> Option<&ToolCall> {
        if let ToolOutput::Content { tool_call, .. } = self {
            Some(tool_call)
        } else {
            None
        }
    }

    pub fn content(&self) -> Option<&str> {
        if let ToolOutput::Content { content, .. } = self {
            Some(content)
        } else {
            None
        }
    }
}

/// TODO: Needs more values, i.e. OpenAI needs a reference to the original call
#[derive(Clone, Builder)]
#[builder(setter(into, strip_option))]
pub struct ToolCall {
    id: String,
    name: String,
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
