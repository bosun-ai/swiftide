//! Wire-format adapters for the shared Chat Completions executor.
//!
//! Keep provider-specific serialization and decoding here and in the provider module. Adapters
//! do not send requests, assemble tool streams, or report usage. They define how wire-specific
//! reasoning fragments combine inside the shared stream accumulator.

use async_openai::types::chat::{
    ChatCompletionMessageToolCallChunk, ChatCompletionMessageToolCalls, CompletionUsage,
    CreateChatCompletionRequest, CreateChatCompletionResponse, CreateChatCompletionStreamResponse,
};
use schemars::Schema;
use serde::de::DeserializeOwned;
use serde_json::Value;
use swiftide_core::ChatCompletionStream;
use swiftide_core::chat_completion::{
    ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ReasoningItem, ToolCall, Usage,
    errors::LanguageModelError,
};

use super::compatible_reasoning;
use super::mistral::MistralAdapter;
use super::{ChatCompletionFormat, GenericOpenAI};

// Decode inside async-openai's BYOT transport so schema errors retain the original response
// body for deterministic-error classification and usage recovery. Normalize only after decoding.
pub(super) trait ChatCompletionAdapter {
    type Completion: DeserializeOwned + Into<CompletionData>;
    type Chunk: DeserializeOwned + Into<DeltaData> + Send + 'static;

    fn request_body(
        request: CreateChatCompletionRequest,
        messages: &[ChatMessage],
    ) -> Result<Value, LanguageModelError>;

    fn accumulate_reasoning(accumulated: &mut Vec<ReasoningItem>, items: &[ReasoningItem]) {
        // SSE boundaries are not reasoning boundaries, even when a block is marked closed.
        for item in items {
            if let Some(previous) = accumulated.last_mut() {
                let text = previous.content.get_or_insert_with(Vec::new);
                if text.is_empty() {
                    text.push(String::new());
                }
                for part in item.content.iter().flatten() {
                    text.last_mut().expect("nonempty").push_str(part);
                }
                previous.status = item.status;
            } else {
                accumulated.push(item.clone());
            }
        }
    }
}

struct OpenAIAdapter;

impl ChatCompletionAdapter for OpenAIAdapter {
    type Completion = compatible_reasoning::Completion;
    type Chunk = compatible_reasoning::Chunk;

    fn request_body(
        request: CreateChatCompletionRequest,
        messages: &[ChatMessage],
    ) -> Result<Value, LanguageModelError> {
        let mut body = serde_json::to_value(request).map_err(LanguageModelError::permanent)?;
        body["messages"] = compatible_reasoning::messages_to_openai(messages)?.into();
        Ok(body)
    }

    fn accumulate_reasoning(accumulated: &mut Vec<ReasoningItem>, items: &[ReasoningItem]) {
        compatible_reasoning::accumulate_reasoning(accumulated, items);
    }
}

impl ChatCompletionFormat {
    pub(super) fn validate_api(self, responses_api: bool) -> Result<(), LanguageModelError> {
        match (self, responses_api) {
            (Self::Mistral, true) => Err(LanguageModelError::permanent(
                "Mistral Chat Completions format cannot be combined with use_responses_api",
            )),
            _ => Ok(()),
        }
    }

    pub(super) async fn complete<C>(
        self,
        client: &GenericOpenAI<C>,
        request: &ChatCompletionRequest<'_>,
        schema: Option<Schema>,
    ) -> Result<ChatCompletionResponse, LanguageModelError>
    where
        C: async_openai::config::Config + Default + Sync + Send + std::fmt::Debug + Clone + 'static,
    {
        match self {
            Self::OpenAI => client.complete_chat::<OpenAIAdapter>(request, schema).await,
            Self::Mistral => {
                client
                    .complete_chat::<MistralAdapter>(request, schema)
                    .await
            }
        }
    }

    pub(super) async fn complete_stream<C>(
        self,
        client: &GenericOpenAI<C>,
        request: &ChatCompletionRequest<'_>,
    ) -> Result<ChatCompletionStream, LanguageModelError>
    where
        C: async_openai::config::Config + Default + Sync + Send + std::fmt::Debug + Clone + 'static,
    {
        match self {
            Self::OpenAI => client.stream_chat::<OpenAIAdapter>(request).await,
            Self::Mistral => client.stream_chat::<MistralAdapter>(request).await,
        }
    }
}

#[derive(Default)]
pub(super) struct ContentData {
    pub text: Option<String>,
    pub reasoning: Option<Vec<ReasoningItem>>,
}

#[derive(Default)]
pub(super) struct CompletionData {
    pub content: ContentData,
    pub tool_calls: Option<Vec<ChatCompletionMessageToolCalls>>,
    pub usage: Option<CompletionUsage>,
}

impl From<CreateChatCompletionResponse> for CompletionData {
    fn from(response: CreateChatCompletionResponse) -> Self {
        let mut result = Self {
            usage: response.usage,
            ..Default::default()
        };
        if let Some(choice) = response.choices.into_iter().next() {
            result.content.text = choice.message.content;
            result.tool_calls = choice.message.tool_calls;
        }
        result
    }
}

impl From<CompletionData> for ChatCompletionResponse {
    fn from(response: CompletionData) -> Self {
        Self {
            message: response.content.text,
            reasoning: response.content.reasoning,
            tool_calls: response.tool_calls.map(|calls| {
                calls
                    .into_iter()
                    .filter_map(|call| match call {
                        ChatCompletionMessageToolCalls::Function(call) => Some(
                            ToolCall::builder()
                                .id(call.id)
                                .name(call.function.name)
                                .args(call.function.arguments)
                                .build()
                                .expect("infallible"),
                        ),
                        ChatCompletionMessageToolCalls::Custom(_) => None,
                    })
                    .collect()
            }),
            usage: response.usage.as_ref().map(Usage::from),
            ..Default::default()
        }
    }
}

#[derive(Default)]
pub(super) struct DeltaData {
    pub content: ContentData,
    pub tool_calls: Option<Vec<ChatCompletionMessageToolCallChunk>>,
    pub usage: Option<CompletionUsage>,
}

impl From<CreateChatCompletionStreamResponse> for DeltaData {
    fn from(chunk: CreateChatCompletionStreamResponse) -> Self {
        let mut result = Self {
            usage: chunk.usage,
            ..Default::default()
        };
        if let Some(choice) = chunk.choices.into_iter().next() {
            result.content.text = choice.delta.content;
            result.tool_calls = choice.delta.tool_calls;
        }
        result
    }
}

impl DeltaData {
    pub(super) fn apply<A: ChatCompletionAdapter>(
        self,
        state: &mut ChatCompletionResponse,
    ) -> Option<Vec<ReasoningItem>> {
        // Tool buffers persist between frames; visible text deltas do not.
        if let Some(delta) = &mut state.delta {
            delta.message_chunk = None;
        }
        state.append_message_delta(self.content.text.as_deref());
        if let Some(items) = &self.content.reasoning {
            A::accumulate_reasoning(state.reasoning.get_or_insert_with(Vec::new), items);
        }
        for call in self.tool_calls.into_iter().flatten() {
            state.append_tool_call_delta(
                call.index as usize,
                call.id.as_deref(),
                call.function.as_ref().and_then(|f| f.name.as_deref()),
                call.function.as_ref().and_then(|f| f.arguments.as_deref()),
            );
        }
        if let Some(usage) = &self.usage {
            // Usage describes the entire request, not a delta. Keep the latest breakdown.
            state.usage = Some(Usage::from(usage));
        }
        self.content.reasoning
    }
}
