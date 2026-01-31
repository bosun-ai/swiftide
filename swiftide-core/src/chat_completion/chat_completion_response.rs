use std::collections::HashMap;

use derive_builder::Builder;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{ReasoningItem, ToolCallBuilder, tools::ToolCall};

/// A generic response from chat completions
///
/// When streaming, the delta is available. Every response will have the accumulated message if
/// present. The final message will also have the final tool calls.
#[derive(Clone, Builder, Debug, Serialize, Deserialize, PartialEq)]
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

    #[builder(default)]
    pub usage: Option<Usage>,

    #[builder(default)]
    pub reasoning: Option<Vec<ReasoningItem>>,

    /// Streaming response
    #[builder(default)]
    pub delta: Option<ChatCompletionResponseDelta>,
}

impl Default for ChatCompletionResponse {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            message: None,
            tool_calls: None,
            delta: None,
            usage: None,
            reasoning: None,
        }
    }
}

/// Usage statistics for a language model response.
#[derive(Clone, Default, Builder, Debug, Serialize, Deserialize, PartialEq)]
#[allow(clippy::struct_field_names)]
pub struct Usage {
    /// Tokens used in the prompt or input.
    pub prompt_tokens: u32,
    /// Tokens generated in the completion or output.
    pub completion_tokens: u32,
    /// Total tokens used for the request.
    pub total_tokens: u32,
    /// Provider-specific usage breakdowns, when available.
    #[builder(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<UsageDetails>,
}

impl Usage {
    pub fn builder() -> UsageBuilder {
        UsageBuilder::default()
    }

    /// Returns a normalized view of usage details when available.
    ///
    /// This keeps the public `Usage` fields intact and derives a consistent input/output breakdown
    /// across providers (e.g. `OpenAI` chat vs. responses). Missing data is left as `None`.
    pub fn normalized(&self) -> NormalizedUsage {
        let details = self.details.as_ref().map(|details| {
            let input = NormalizedInputUsageDetails {
                cached_tokens: details
                    .input_tokens_details
                    .as_ref()
                    .and_then(|input| input.cached_tokens)
                    .or_else(|| {
                        details
                            .prompt_tokens_details
                            .as_ref()
                            .and_then(|prompt| prompt.cached_tokens)
                    }),
                audio_tokens: details
                    .prompt_tokens_details
                    .as_ref()
                    .and_then(|prompt| prompt.audio_tokens),
            };
            let output = NormalizedOutputUsageDetails {
                reasoning_tokens: details
                    .output_tokens_details
                    .as_ref()
                    .and_then(|output| output.reasoning_tokens)
                    .or_else(|| {
                        details
                            .completion_tokens_details
                            .as_ref()
                            .and_then(|completion| completion.reasoning_tokens)
                    }),
                audio_tokens: details
                    .completion_tokens_details
                    .as_ref()
                    .and_then(|completion| completion.audio_tokens),
                accepted_prediction_tokens: details
                    .completion_tokens_details
                    .as_ref()
                    .and_then(|completion| completion.accepted_prediction_tokens),
                rejected_prediction_tokens: details
                    .completion_tokens_details
                    .as_ref()
                    .and_then(|completion| completion.rejected_prediction_tokens),
            };

            if input.is_empty() && output.is_empty() {
                None
            } else {
                Some(NormalizedUsageDetails { input, output })
            }
        });

        NormalizedUsage {
            prompt_tokens: self.prompt_tokens,
            completion_tokens: self.completion_tokens,
            total_tokens: self.total_tokens,
            details: details.flatten(),
        }
    }
}

/// Provider-specific usage breakdowns for a response.
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct UsageDetails {
    /// Chat-completions style prompt token details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_details: Option<PromptTokensDetails>,
    /// Chat-completions style completion token details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_tokens_details: Option<CompletionTokensDetails>,
    /// Responses-style input token details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens_details: Option<InputTokenDetails>,
    /// Responses-style output token details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens_details: Option<OutputTokenDetails>,
}

/// Normalized usage totals with optional normalized details.
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct NormalizedUsage {
    /// Tokens used in the prompt or input.
    pub prompt_tokens: u32,
    /// Tokens generated in the completion or output.
    pub completion_tokens: u32,
    /// Total tokens used for the request.
    pub total_tokens: u32,
    /// Normalized input/output breakdown, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<NormalizedUsageDetails>,
}

/// Normalized input/output usage breakdown.
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct NormalizedUsageDetails {
    /// Normalized input usage details.
    pub input: NormalizedInputUsageDetails,
    /// Normalized output usage details.
    pub output: NormalizedOutputUsageDetails,
}

/// Normalized input usage details.
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct NormalizedInputUsageDetails {
    /// Tokens retrieved from cache, when provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u32>,
    /// Audio tokens in the input, when provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_tokens: Option<u32>,
}

impl NormalizedInputUsageDetails {
    fn is_empty(&self) -> bool {
        self.cached_tokens.is_none() && self.audio_tokens.is_none()
    }
}

/// Normalized output usage details.
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct NormalizedOutputUsageDetails {
    /// Tokens used for reasoning, when provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u32>,
    /// Audio tokens in the output, when provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_tokens: Option<u32>,
    /// Accepted prediction tokens, when provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted_prediction_tokens: Option<u32>,
    /// Rejected prediction tokens, when provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejected_prediction_tokens: Option<u32>,
}

impl NormalizedOutputUsageDetails {
    fn is_empty(&self) -> bool {
        self.reasoning_tokens.is_none()
            && self.audio_tokens.is_none()
            && self.accepted_prediction_tokens.is_none()
            && self.rejected_prediction_tokens.is_none()
    }
}

/// OpenAI-style prompt token details (chat completions).
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct PromptTokensDetails {
    /// Audio input tokens present in the prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_tokens: Option<u32>,
    /// Cached tokens present in the prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u32>,
}

#[cfg(feature = "openai")]
impl PromptTokensDetails {
    fn is_empty(&self) -> bool {
        self.audio_tokens.is_none() && self.cached_tokens.is_none()
    }
}

/// OpenAI-style completion token details (chat completions).
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct CompletionTokensDetails {
    /// Tokens accepted from predicted output, when provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted_prediction_tokens: Option<u32>,
    /// Audio tokens generated by the model, when provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_tokens: Option<u32>,
    /// Tokens generated by the model for reasoning, when provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u32>,
    /// Tokens rejected from predicted output, when provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejected_prediction_tokens: Option<u32>,
}

#[cfg(feature = "openai")]
impl CompletionTokensDetails {
    fn is_empty(&self) -> bool {
        self.accepted_prediction_tokens.is_none()
            && self.audio_tokens.is_none()
            && self.reasoning_tokens.is_none()
            && self.rejected_prediction_tokens.is_none()
    }
}

/// OpenAI-style input token details (Responses API).
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct InputTokenDetails {
    /// Tokens retrieved from cache, when provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u32>,
}

/// OpenAI-style output token details (Responses API).
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct OutputTokenDetails {
    /// Tokens used for reasoning, when provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u32>,
}

#[cfg(feature = "openai")]
mod openai_usage {
    use super::{
        CompletionTokensDetails, InputTokenDetails, OutputTokenDetails, PromptTokensDetails, Usage,
        UsageDetails,
    };
    use async_openai::types::{
        chat::CompletionUsage, embeddings::EmbeddingUsage, responses::ResponseUsage,
    };

    impl From<&CompletionUsage> for Usage {
        fn from(usage: &CompletionUsage) -> Self {
            let prompt_details = usage.prompt_tokens_details.as_ref().and_then(|details| {
                let details = PromptTokensDetails {
                    audio_tokens: details.audio_tokens,
                    cached_tokens: details.cached_tokens,
                };
                if details.is_empty() {
                    None
                } else {
                    Some(details)
                }
            });
            let completion_details = usage
                .completion_tokens_details
                .as_ref()
                .and_then(|details| {
                    let details = CompletionTokensDetails {
                        accepted_prediction_tokens: details.accepted_prediction_tokens,
                        audio_tokens: details.audio_tokens,
                        reasoning_tokens: details.reasoning_tokens,
                        rejected_prediction_tokens: details.rejected_prediction_tokens,
                    };
                    if details.is_empty() {
                        None
                    } else {
                        Some(details)
                    }
                });
            let details = if prompt_details.is_some() || completion_details.is_some() {
                Some(UsageDetails {
                    prompt_tokens_details: prompt_details,
                    completion_tokens_details: completion_details,
                    input_tokens_details: None,
                    output_tokens_details: None,
                })
            } else {
                None
            };

            Usage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
                details,
            }
        }
    }

    impl From<&ResponseUsage> for Usage {
        fn from(usage: &ResponseUsage) -> Self {
            Usage {
                prompt_tokens: usage.input_tokens,
                completion_tokens: usage.output_tokens,
                total_tokens: usage.total_tokens,
                details: Some(UsageDetails {
                    prompt_tokens_details: None,
                    completion_tokens_details: None,
                    input_tokens_details: Some(InputTokenDetails {
                        cached_tokens: Some(usage.input_tokens_details.cached_tokens),
                    }),
                    output_tokens_details: Some(OutputTokenDetails {
                        reasoning_tokens: Some(usage.output_tokens_details.reasoning_tokens),
                    }),
                }),
            }
        }
    }

    impl From<&EmbeddingUsage> for Usage {
        fn from(usage: &EmbeddingUsage) -> Self {
            Usage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: 0,
                total_tokens: usage.total_tokens,
                details: None,
            }
        }
    }
}

#[derive(Clone, Builder, Debug, Serialize, Deserialize, PartialEq)]
pub struct ChatCompletionResponseDelta {
    #[builder(default)]
    pub message_chunk: Option<String>,

    #[builder(default)]
    pub tool_calls_chunk: Option<HashMap<usize, ToolCallAccum>>,
}

// Accumulator for streamed tool calls
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolCallAccum {
    pub id: Option<String>,
    pub name: Option<String>,
    pub arguments: Option<String>,
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
        index: usize,
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

    pub fn append_usage_delta(
        &mut self,
        prompt_tokens: u32,
        completion_tokens: u32,
        total_tokens: u32,
    ) -> &mut Self {
        debug_assert!(prompt_tokens + completion_tokens == total_tokens);

        if let Some(usage) = &mut self.usage {
            usage.prompt_tokens += prompt_tokens;
            usage.completion_tokens += completion_tokens;
            usage.total_tokens += total_tokens;
        } else {
            self.usage = Some(Usage {
                prompt_tokens,
                completion_tokens,
                total_tokens,
                details: None,
            });
        }
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
