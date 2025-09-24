//! Use tiktoken-rs to estimate token count on various common Swiftide types
//!
//! Intended to be used for openai models.
//!
//! Note that the library is heavy on the unwraps.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::tokenizer::{Estimatable, EstimateTokens};
use tiktoken_rs::{CoreBPE, get_bpe_from_model, get_bpe_from_tokenizer, tokenizer::Tokenizer};

/// A tiktoken based tokenizer for openai models. Can also be used for other models.
///
/// Implements `EstimateTokens` for various swiftide types (prompts, chat messages, lists of chat
/// messages) and regular strings.
///
/// Estimates are estimates; not exact counts.
///
/// # Example
///
/// ```no_run
/// # use swiftide_core::tokenizer::EstimateTokens;
/// # use swiftide_integrations::tiktoken::TikToken;
///
/// # async fn test() {
/// let tokenizer = TikToken::try_from_model("gpt-4-0314").unwrap();
/// let estimate = tokenizer.estimate("hello {{world}}").await.unwrap();
///
/// assert_eq!(estimate, 4);
/// # }
/// ```
#[derive(Clone)]
pub struct TikToken {
    /// The tiktoken model to use
    bpe: Arc<CoreBPE>,
}

impl std::fmt::Debug for TikToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TikToken").finish()
    }
}

impl Default for TikToken {
    fn default() -> Self {
        Self::try_from_model("gpt-4o")
            .expect("infallible; gpt-4o should be valid model for tiktoken")
    }
}

impl TikToken {
    /// Build a `TikToken` from an openai model name
    ///
    /// # Errors
    ///
    /// Errors if the tokenizer cannot be found from the model or it cannot be build
    pub fn try_from_model(model: impl AsRef<str>) -> Result<Self> {
        let bpe = get_bpe_from_model(model.as_ref())?;
        Ok(Self { bpe: Arc::new(bpe) })
    }

    /// Build a `TikToken` from a `tiktoken_rs::tiktoken::Tokenizer`
    ///
    /// # Errors
    ///
    /// Errors if the tokenizer cannot be build
    pub fn try_from_tokenizer(tokenizer: Tokenizer) -> Result<Self> {
        let bpe = get_bpe_from_tokenizer(tokenizer)?;
        Ok(Self { bpe: Arc::new(bpe) })
    }
}

#[async_trait]
impl EstimateTokens for TikToken {
    async fn estimate(&self, value: impl Estimatable) -> Result<usize> {
        let mut total = 0;
        for text in value.for_estimate()? {
            total += self.bpe.encode_with_special_tokens(text.as_ref()).len();
        }

        Ok(total + value.additional_tokens())
    }
}

#[cfg(test)]
mod tests {
    use swiftide_core::{chat_completion::ChatMessage, prompt::Prompt};

    use super::*;

    #[tokio::test]
    async fn test_estimate_tokens() {
        let tokenizer = TikToken::try_from_model("gpt-4-0314").unwrap();
        let prompt = Prompt::from("hello {{world}}");
        let tokens = tokenizer.estimate(&prompt).await.unwrap();
        assert_eq!(tokens, 4);
    }

    #[tokio::test]
    async fn test_estimate_tokens_from_tokenizer() {
        let tokenizer = TikToken::try_from_tokenizer(Tokenizer::O200kBase).unwrap();
        let prompt = "hello {{world}}";
        let tokens = tokenizer.estimate(prompt).await.unwrap();
        assert_eq!(tokens, 4);
    }

    #[tokio::test]
    async fn test_estimate_chat_messages() {
        let messages = vec![
            ChatMessage::new_user("hello ".repeat(10)),
            ChatMessage::new_system("world"),
        ];

        // 11x hello + 1x world + 2x 4 per message + 1x 3 for full + 2 whatever = 23

        let tokenizer = TikToken::try_from_model("gpt-4-0314").unwrap();
        dbg!(messages.as_slice().for_estimate().await.unwrap());

        assert_eq!(tokenizer.estimate(messages.as_slice()).await.unwrap(), 23);
    }
}
