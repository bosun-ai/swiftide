use std::borrow::Cow;

use anyhow::Result;
use async_trait::async_trait;

use crate::{chat_completion::ChatMessage, prompt::Prompt};

/// Estimate the number of tokens in a given value.
///
/// This trait is intentionally async so implementations can defer to remote or
/// more expensive estimators without blocking.
///
/// # Examples
///
/// ```rust
/// # use swiftide_core::token_estimation::{CharEstimator, EstimateTokens};
/// # use swiftide_core::chat_completion::ChatMessage;
/// # #[tokio::main]
/// # async fn main() -> anyhow::Result<()> {
/// let estimator = CharEstimator;
/// let message = ChatMessage::new_user("Hello from Swiftide!");
/// let tokens = estimator.estimate(&message).await?;
/// assert!(tokens > 0);
/// # Ok(())
/// # }
/// ```
#[async_trait]
pub trait EstimateTokens {
    async fn estimate(&self, value: impl Estimatable) -> Result<usize>;
}

/// A rough estimator when speed matters more than accuracy.
///
/// Divides the number of characters by 4 as recommended by `OpenAI`.
///
/// # Examples
///
/// ```rust
/// # use swiftide_core::token_estimation::{CharEstimator, EstimateTokens};
/// # #[tokio::main]
/// # async fn main() -> anyhow::Result<()> {
/// let estimator = CharEstimator;
/// let tokens = estimator.estimate("Roughly four chars per token.").await?;
/// assert!(tokens > 0);
/// # Ok(())
/// # }
/// ```
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
///
/// # Examples
///
/// ```rust
/// # use std::borrow::Cow;
/// # use anyhow::Result;
/// # use swiftide_core::token_estimation::Estimatable;
/// struct Snippet {
///     title: String,
///     body: String,
/// }
///
/// impl Estimatable for Snippet {
///     fn for_estimate(&self) -> Result<Vec<Cow<'_, str>>> {
///         Ok(vec![Cow::Borrowed(&self.title), Cow::Borrowed(&self.body)])
///     }
/// }
/// ```
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_completion::ToolCall;

    #[tokio::test]
    async fn estimate_counts_characters_and_additional_tokens() {
        let estimator = CharEstimator;
        let tokens = estimator.estimate("abcd").await.unwrap();
        assert_eq!(tokens, 1);
    }

    #[tokio::test]
    async fn estimate_prompt_renders_before_counting() {
        let estimator = CharEstimator;
        let prompt = Prompt::from("hello {{name}}").with_context_value("name", "swiftide");
        let tokens = estimator.estimate(&prompt).await.unwrap();
        assert_eq!(tokens, "hello swiftide".chars().count() / 4);
    }

    #[tokio::test]
    async fn estimate_chat_message_includes_role_tokens() {
        let estimator = CharEstimator;
        let message = ChatMessage::new_user("hello");
        let tokens = estimator.estimate(&message).await.unwrap();
        assert_eq!(tokens, "hello".chars().count() / 4 + 4);
    }

    #[tokio::test]
    async fn estimate_slice_adds_reply_priming_tokens() {
        let estimator = CharEstimator;
        let messages = [
            ChatMessage::new_user("hello"),
            ChatMessage::new_system("world"),
        ];
        let tokens = estimator.estimate(&messages[..]).await.unwrap();
        let content_tokens = "helloworld".chars().count() / 4;
        let additional_tokens = 4 + 4 + 3;
        assert_eq!(tokens, content_tokens + additional_tokens);
    }

    #[tokio::test]
    async fn assistant_tool_calls_are_included_in_estimate() {
        let estimator = CharEstimator;
        let tool_call = ToolCall::builder()
            .id("tool-1")
            .name("search")
            .args("{\"q\":\"swiftide\"}")
            .build()
            .unwrap();
        let message = ChatMessage::new_assistant(None::<String>, Some(vec![tool_call]));
        let tokens = estimator.estimate(&message).await.unwrap();
        let content_tokens = "{\"q\":\"swiftide\"}".chars().count() / 4;
        assert_eq!(tokens, content_tokens + 4);
    }

    #[tokio::test]
    async fn assistant_without_content_or_tools_uses_none_marker() {
        let message = ChatMessage::Assistant(None, None);
        let message_ref = &message;
        let content = message_ref.for_estimate().unwrap();
        assert_eq!(content, vec![Cow::Borrowed("None")]);
    }
}
