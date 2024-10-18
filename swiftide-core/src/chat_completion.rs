use anyhow::Result;
use async_trait::async_trait;
use derive_builder::Builder;
use dyn_clone::DynClone;

use crate::prompt::Prompt;

#[async_trait]
pub trait ChatCompletion: Send + Sync + DynClone {
    async fn complete(
        &self,
        request: impl Into<ChatCompletionRequest<'_>> + Send + Sync,
    ) -> Result<ChatCompletionResponse>;
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

pub enum ChatMessage {
    System(String),
    User(String),
    ToolCall(ToolCall),
    ToolOuput(ToolOutput),
}

pub enum ChatRole {
    System,
    User,
    Assistant,
}

#[derive(Clone)]
pub enum ToolOutput {
    /// Adds the result of the toolcall to messages
    ToolCall {
        tool_call_id: String,
        name: String,
        content: String,
    },
    /// Stops an agent
    ///
    Stop(bool),
    //Raw(String),
    //Agent(Agent),
}

impl ToolOutput {
    pub fn tool_call_id(&self) -> Option<&str> {
        if let ToolOutput::ToolCall { tool_call_id, .. } = self {
            Some(tool_call_id)
        } else {
            None
        }
    }

    pub fn name(&self) -> Option<&str> {
        if let ToolOutput::ToolCall { name, .. } = self {
            Some(name)
        } else {
            None
        }
    }

    pub fn content(&self) -> Option<&str> {
        if let ToolOutput::ToolCall { content, .. } = self {
            Some(content)
        } else {
            None
        }
    }
}

/// TODO: Needs more values, i.e. OpenAI needs a reference to the original call
#[derive(Clone, Builder)]
pub struct ToolCall {
    id: String,
    name: String,
    arguments: String,
}

impl ToolCall {
    pub fn builder() -> ToolCallBuilder {
        ToolCallBuilder::default()
    }
}

pub type JsonSpec = &'static str;
