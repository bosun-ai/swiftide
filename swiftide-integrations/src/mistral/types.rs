//! Mistral completion and stream accumulation. SDK tool and usage types are reused unchanged.
use async_openai::types::chat::{
    ChatCompletionMessageToolCallChunk, ChatCompletionMessageToolCalls, CompletionUsage,
};
use swiftide_core::chat_completion::{ChatCompletionResponse, ReasoningItem, ToolCall, Usage};

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

impl DeltaData {
    pub(super) fn apply(self, state: &mut ChatCompletionResponse) -> Option<Vec<ReasoningItem>> {
        // The core helper clears absent text deltas while preserving tool buffers.
        state.append_message_delta(self.content.text.as_deref());
        if let Some(items) = &self.content.reasoning {
            accumulate_reasoning(state.reasoning.get_or_insert_with(Vec::new), items);
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
