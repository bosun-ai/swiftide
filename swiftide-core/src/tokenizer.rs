use std::borrow::Cow;

use anyhow::Result;
use async_trait::async_trait;

use crate::{chat_completion::ChatMessage, prompt::Prompt};

/// Estimate the number of tokens in a given value.
#[async_trait]
pub trait EstimateTokens {
    async fn estimate(&self, value: impl Estimatable) -> Result<usize>;
}

/// A value that can be estimated for the number of tokens it contains.
#[async_trait]
pub trait Estimatable: Send + Sync {
    async fn for_estimate(&self) -> Result<Cow<'_, str>>;

    /// Optionally return extra tokens that should be added to the estimate.
    fn additional_tokens(&self) -> usize {
        0
    }
}

#[async_trait]
impl Estimatable for &str {
    async fn for_estimate(&self) -> Result<Cow<'_, str>> {
        Ok(Cow::Borrowed(self))
    }
}

#[async_trait]
impl Estimatable for String {
    async fn for_estimate(&self) -> Result<Cow<'_, str>> {
        Ok(Cow::Borrowed(self.as_str()))
    }
}

#[async_trait]
impl Estimatable for &Prompt {
    async fn for_estimate(&self) -> Result<Cow<'_, str>> {
        let rendered = self.render()?;
        Ok(Cow::Owned(rendered))
    }
}

#[async_trait]
impl Estimatable for &ChatMessage {
    async fn for_estimate(&self) -> Result<Cow<'_, str>> {
        Ok(match self {
            ChatMessage::User(msg) | ChatMessage::Summary(msg) | ChatMessage::System(msg) => {
                Cow::Borrowed(msg)
            }
            ChatMessage::Assistant(msg, vec) => {
                // Note that this is not super accurate.
                //
                // It's a bit verbose to avoid unnecessary allocations. Is what it is.
                let tool_calls = vec.as_ref().map(|vec| {
                    vec.iter()
                        .map(std::string::ToString::to_string)
                        .collect::<Vec<String>>()
                        .join(" ")
                });

                if let Some(msg) = msg {
                    if let Some(tool_calls) = tool_calls {
                        format!("{msg} {tool_calls}").into()
                    } else {
                        msg.into()
                    }
                } else if let Some(tool_calls) = tool_calls {
                    tool_calls.into()
                } else {
                    "None".into()
                }
            }
            ChatMessage::ToolOutput(tool_call, tool_output) => {
                let tool_call_id = tool_call.id();
                let tool_output_content = tool_output.content().unwrap_or_default();

                format!("{tool_call_id} {tool_output_content}").into()
            }
        })
    }

    // 4 each for the role
    //
    // See https://github.com/openai/openai-cookbook/blob/main/examples/How_to_count_tokens_with_tiktoken.ipynb
    fn additional_tokens(&self) -> usize {
        4
    }
}

#[async_trait]
impl Estimatable for &[ChatMessage] {
    async fn for_estimate(&self) -> Result<Cow<'_, str>> {
        let mut total = 0;
        for msg in *self {
            total += msg.for_estimate().await?.len();
        }

        Ok(total.to_string().into())
    }

    // Apparently every reply is primed with a <|start|>assistant<|message|>
    fn additional_tokens(&self) -> usize {
        self.iter().map(|m| m.additional_tokens()).sum::<usize>() + 3
    }
}
