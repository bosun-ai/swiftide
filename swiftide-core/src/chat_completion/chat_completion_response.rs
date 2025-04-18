use std::collections::HashMap;

use derive_builder::Builder;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{tools::ToolCall, ToolCallBuilder};

/// A generic response from chat completions
///
/// When streaming, the delta is available. Every response will have the accumulated message if
/// present. The final message will also have the final tool calls.
#[derive(Clone, Builder, Debug, Default, Serialize, Deserialize, PartialEq)]
#[builder(setter(strip_option, into), build_fn(error = anyhow::Error))]
pub struct ChatCompletionResponse {
    /// An identifier for the response
    ///
    /// Useful when streaming to make sure chunks can be mapped to the right response
    #[builder(private, default = Uuid::new_v4())]
    pub id: Uuid,

    #[builder(default)]
    pub message: Option<String>,

    #[builder(default)]
    pub tool_calls: Option<Vec<ToolCall>>,

    /// Streaming response
    #[builder(default)]
    pub delta: Option<ChatCompletionResponseDelta>,
}

#[derive(Clone, Builder, Debug, Serialize, Deserialize, PartialEq)]
pub struct ChatCompletionResponseDelta {
    pub message_chunk: Option<String>,

    // These are not public as the assumption is they are not usable
    // until the tool calls are valid
    tool_calls_chunk: Option<HashMap<u32, ToolCallAccum>>,
}

// Accumulator for streamed tool calls
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct ToolCallAccum {
    id: Option<String>,
    name: Option<String>,
    arguments: Option<String>,
}

impl ChatCompletionResponse {
    pub fn builder() -> ChatCompletionResponseBuilder {
        ChatCompletionResponseBuilder::default()
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub fn tool_calls(&self) -> Option<&[ToolCall]> {
        self.tool_calls.as_deref()
    }

    /// Adds a streaming chunk to the message and also the delta
    pub fn append_message_delta(&mut self, message_delta: Option<&str>) -> &mut Self {
        // let message: Option<String> = message;
        let Some(message_delta) = message_delta else {
            return self;
        };

        if let Some(delta) = &mut self.delta {
            delta.message_chunk = Some(message_delta.to_string());
        } else {
            self.delta = Some(ChatCompletionResponseDelta {
                message_chunk: Some(message_delta.to_string()),
                tool_calls_chunk: None,
            });
        }

        self.message
            .as_mut()
            .map(|m| {
                m.push_str(message_delta);
            })
            .unwrap_or_else(|| {
                self.message = Some(message_delta.to_string());
            });
        self
    }

    /// Adds a streaming chunk to the tool calls, if it can be build, the tool call will be build,
    /// otherwise it will remain in the delta and retried on the next call
    pub fn append_tool_call_delta(
        &mut self,
        index: u32,
        id: Option<&str>,
        name: Option<&str>,
        arguments: Option<&str>,
    ) -> &mut Self {
        if let Some(delta) = &mut self.delta {
            let map = delta.tool_calls_chunk.get_or_insert_with(HashMap::new);
            map.entry(index)
                .and_modify(|v| {
                    if v.id.is_none() {
                        v.id = id.map(Into::into);
                    }
                    if v.name.is_none() {
                        v.name = name.map(Into::into);
                    }
                    if let Some(v) = v.arguments.as_mut() {
                        if let Some(arguments) = arguments {
                            v.push_str(arguments);
                        }
                    } else {
                        v.arguments = arguments.map(Into::into);
                    }
                })
                .or_insert(ToolCallAccum {
                    id: id.map(Into::into),
                    name: name.map(Into::into),
                    arguments: arguments.map(Into::into),
                });
        } else {
            self.delta = Some(ChatCompletionResponseDelta {
                message_chunk: None,
                tool_calls_chunk: Some(HashMap::from([(
                    index,
                    ToolCallAccum {
                        id: id.map(Into::into),
                        name: name.map(Into::into),
                        arguments: arguments.map(Into::into),
                    },
                )])),
            });
        }

        // Now let's try to rebuild _every_ tool call and overwrite
        // Performance wise very meh but it works, in reality it's only a couple of tool calls most
        self.finalize_tools_from_stream();

        self
    }

    fn finalize_tools_from_stream(&mut self) {
        if let Some(values) = self
            .delta
            .as_ref()
            .and_then(|d| d.tool_calls_chunk.as_ref().map(|t| t.values()))
        {
            let maybe_tool_calls = values
                .filter_map(|maybe_tool_call| {
                    ToolCallBuilder::default()
                        .maybe_id(maybe_tool_call.id.clone())
                        .maybe_name(maybe_tool_call.name.clone())
                        .maybe_args(maybe_tool_call.arguments.clone())
                        .build()
                        .ok()
                })
                .collect::<Vec<_>>();

            if !maybe_tool_calls.is_empty() {
                self.tool_calls = Some(maybe_tool_calls);
            }
        }
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
