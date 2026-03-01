use std::{borrow::Cow, collections::HashSet};

use derive_builder::Builder;

use super::{chat_message::ChatMessage, tools::ToolSpec, traits::Tool};

/// A chat completion request represents a series of chat messages and tool interactions that can
/// be send to any LLM.
#[derive(Builder, Clone, PartialEq, Debug)]
#[builder(setter(into, strip_option))]
pub struct ChatCompletionRequest<'a> {
    pub messages: Cow<'a, [ChatMessage]>,
    #[builder(default, setter(custom))]
    pub tools_spec: HashSet<ToolSpec>,
}

impl<'a> ChatCompletionRequest<'a> {
    pub fn builder() -> ChatCompletionRequestBuilder<'a> {
        ChatCompletionRequestBuilder::default()
    }

    /// Returns the chat messages included in the request.
    pub fn messages(&self) -> &[ChatMessage] {
        self.messages.as_ref()
    }

    /// Returns the tool specifications currently attached to the request.
    pub fn tools_spec(&self) -> &HashSet<ToolSpec> {
        &self.tools_spec
    }

    /// Returns an owned request with `'static` data.
    pub fn to_owned(&self) -> ChatCompletionRequest<'static> {
        ChatCompletionRequest {
            messages: Cow::Owned(self.messages.iter().map(ChatMessage::to_owned).collect()),
            tools_spec: self.tools_spec.clone(),
        }
    }
}

impl From<Vec<ChatMessage>> for ChatCompletionRequest<'_> {
    fn from(messages: Vec<ChatMessage>) -> Self {
        ChatCompletionRequest {
            messages: Cow::Owned(messages),
            tools_spec: HashSet::new(),
        }
    }
}

impl<'a> From<&'a [ChatMessage]> for ChatCompletionRequest<'a> {
    fn from(messages: &'a [ChatMessage]) -> Self {
        ChatCompletionRequest {
            messages: Cow::Borrowed(messages),
            tools_spec: HashSet::new(),
        }
    }
}

impl ChatCompletionRequestBuilder<'_> {
    #[deprecated(note = "Use `tools` with real Tool instances instead")]
    pub fn tools_spec(&mut self, tools_spec: HashSet<ToolSpec>) -> &mut Self {
        self.tools_spec = Some(tools_spec);
        self
    }

    /// Adds multiple tools by deriving their specs from the provided instances.
    pub fn tools<I, T>(&mut self, tools: I) -> &mut Self
    where
        I: IntoIterator<Item = T>,
        T: Into<Box<dyn Tool>>,
    {
        let specs = tools.into_iter().map(|tool| {
            let boxed: Box<dyn Tool> = tool.into();
            boxed.tool_spec()
        });
        self.tool_specs(specs)
    }

    /// Adds a single tool instance to the request by deriving its spec.
    pub fn tool<T>(&mut self, tool: T) -> &mut Self
    where
        T: Into<Box<dyn Tool>>,
    {
        let boxed: Box<dyn Tool> = tool.into();
        self.tool_specs(std::iter::once(boxed.tool_spec()))
    }

    /// Extends the request with additional tool specifications.
    pub fn tool_specs<I>(&mut self, specs: I) -> &mut Self
    where
        I: IntoIterator<Item = ToolSpec>,
    {
        let entry = self.tools_spec.get_or_insert_with(HashSet::new);
        entry.extend(specs);
        self
    }
}
