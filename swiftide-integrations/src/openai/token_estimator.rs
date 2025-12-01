use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::{Estimatable, chat_completion::ChatMessage, tokenizer::EstimateTokens};

/// Object-safe token estimator used by overflow handling.
#[async_trait]
pub trait TokenEstimator: Send + Sync {
    async fn estimate_chat_messages(&self, messages: &[ChatMessage]) -> Result<usize>;
}

/// Adapter that turns any `EstimateTokens` implementor into a `TokenEstimator`.
#[derive(Clone)]
pub(crate) struct CoreEstimator<E: EstimateTokens + Send + Sync + 'static>(pub E);

#[async_trait]
impl<E: EstimateTokens + Send + Sync + 'static> TokenEstimator for CoreEstimator<E> {
    async fn estimate_chat_messages(&self, messages: &[ChatMessage]) -> Result<usize> {
        self.0.estimate(messages).await
    }
}

/// A lightweight character-based estimator used when no tokenizer is provided.
/// It assumes roughly 4 characters per token.
#[derive(Clone, Default, Debug)]
pub(crate) struct CharEstimator;

#[async_trait]
impl EstimateTokens for CharEstimator {
    async fn estimate(&self, value: impl Estimatable) -> Result<usize> {
        let s = value.for_estimate().await?;
        let base = (s.chars().count() + 3) / 4;
        Ok(base + value.additional_tokens())
    }
}

/// Provide the default token estimator, preferring tiktoken when available.
pub(crate) fn default_token_estimator() -> Arc<dyn TokenEstimator + Send + Sync> {
    #[cfg(feature = "tiktoken")]
    {
        Arc::new(CoreEstimator(crate::tiktoken::TikToken::default()))
    }

    #[cfg(not(feature = "tiktoken"))]
    {
        Arc::new(CoreEstimator(CharEstimator::default()))
    }
}
