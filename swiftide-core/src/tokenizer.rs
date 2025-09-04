use std::borrow::Cow;

use anyhow::Result;
use async_trait::async_trait;

use crate::{chat_completion::ChatMessage, prompt::Prompt};

/// Estimate the number of tokens in a given value.
#[async_trait]
pub trait EstimateTokens {
    async fn estimate(&self, value: impl Estimatable) -> Result<usize>;
}

/// A rough estimater when speed matters more than accuracy.
///
/// Devides the number of characters by 4 as recommended by `OpenAI`.
pub struct CharEstimator;

#[async_trait]
impl EstimateTokens for CharEstimator {
    async fn estimate(&self, value: impl Estimatable) -> Result<usize> {
        let s = value.for_estimate()?;
        Ok(s.iter().map(|s| s.chars().count()).sum::<usize>() / 4 + value.additional_tokens())
    }
}

/// A value that can be estimated for the number of tokens it contains.
///
/// # Errors
///
/// Errors if the value cannot be presented for estimation.
pub trait Estimatable: Send + Sync {
    fn for_estimate(&self) -> Result<Vec<Cow<'_, str>>>;

    /// Optionally return extra tokens that should be added to the estimate.
    fn additional_tokens(&self) -> usize {
        0
    }
}

impl Estimatable for &str {
    fn for_estimate(&self) -> Result<Vec<Cow<'_, str>>> {
        Ok(vec![Cow::Borrowed(self)])
    }
}

impl Estimatable for String {
    fn for_estimate(&self) -> Result<Vec<Cow<'_, str>>> {
        Ok(vec![Cow::Borrowed(self.as_str())])
    }
}

impl Estimatable for &Prompt {
    fn for_estimate(&self) -> Result<Vec<Cow<'_, str>>> {
        let rendered = self.render()?;
        Ok(vec![Cow::Owned(rendered)])
    }
}

impl Estimatable for &ChatMessage {
    fn for_estimate(&self) -> Result<Vec<Cow<'_, str>>> {
        Ok(match self {
            ChatMessage::User(msg) | ChatMessage::Summary(msg) | ChatMessage::System(msg) => {
                vec![Cow::Borrowed(msg)]
            }
            ChatMessage::Assistant(msg, vec) => {
                // Note that this is not super accurate.
                //
                // It's a bit verbose to avoid unnecessary allocations. Is what it is.
                let mut tool_calls = vec.as_ref().map(|vec| {
                    vec.iter()
                        .filter_map(|c| c.args().map(Cow::Borrowed))
                        .collect::<Vec<_>>()
                });

                if let Some(msg) = msg {
                    if let Some(tool_calls) = tool_calls.as_mut() {
                        let mut msg = vec![Cow::Borrowed(msg.as_str())];
                        msg.append(tool_calls);
                        msg
                    } else {
                        vec![Cow::Borrowed(msg)]
                    }
                } else if let Some(tool_calls) = tool_calls {
                    tool_calls
                } else {
                    vec!["None".into()]
                }
            }
            ChatMessage::ToolOutput(_tool_call, tool_output) => {
                let tool_output_content = tool_output.content().unwrap_or_default();

                vec![Cow::Borrowed(tool_output_content)]
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

impl Estimatable for &[ChatMessage] {
    fn for_estimate(&self) -> Result<Vec<Cow<'_, str>>> {
        let mut total = Vec::new();
        for msg in *self {
            let mut v = msg
                .for_estimate()?
                .into_iter()
                .map(Cow::into_owned)
                .map(Into::into)
                .collect();
            total.append(&mut v);
        }

        Ok(total)
    }

    // Apparently every reply is primed with a <|start|>assistant<|message|>
    fn additional_tokens(&self) -> usize {
        self.iter().map(|m| m.additional_tokens()).sum::<usize>() + 3
    }
}
