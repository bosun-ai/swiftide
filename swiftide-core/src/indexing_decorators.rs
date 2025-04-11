use std::fmt::Debug;

use crate::{prompt::Prompt, EmbeddingModel, Embeddings, SimplePrompt};

use crate::chat_completion::errors::LanguageModelError;
use anyhow::Result;
use async_trait::async_trait;
use std::time::Duration;

/// Backoff configuration for api calls.
/// Each time an api call fails backoff will wait an increasing period of time for each subsequent
/// retry attempt. see <https://docs.rs/backoff/latest/backoff/> for more details.
#[derive(Debug, Clone, Copy)]
pub struct BackoffConfiguration {
    /// Initial interval in seconds between retries
    pub initial_interval_sec: u64,
    /// The factor by which the interval is multiplied on each retry attempt
    pub multiplier: f64,
    /// Introduces randomness to avoid retry storms
    pub randomization_factor: f64,
    /// Total time all attempts are allowed in seconds. Once a retry must wait longer than this,
    /// the request is considered to have failed.
    pub max_elapsed_time_sec: u64,
}

impl Default for BackoffConfiguration {
    fn default() -> Self {
        Self {
            initial_interval_sec: 1,
            multiplier: 2.0,
            randomization_factor: 0.5,
            max_elapsed_time_sec: 60,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LanguageModelWithBackOff<P: Clone> {
    pub(crate) inner: P,
    config: BackoffConfiguration,
}

impl<P: Clone> LanguageModelWithBackOff<P> {
    pub fn new(client: P, config: BackoffConfiguration) -> Self {
        Self {
            inner: client,
            config,
        }
    }

    pub(crate) fn strategy(&self) -> backoff::ExponentialBackoff {
        backoff::ExponentialBackoffBuilder::default()
            .with_initial_interval(Duration::from_secs(self.config.initial_interval_sec))
            .with_multiplier(self.config.multiplier)
            .with_max_elapsed_time(Some(Duration::from_secs(self.config.max_elapsed_time_sec)))
            .with_randomization_factor(self.config.randomization_factor)
            .build()
    }
}

#[async_trait]
impl<P: SimplePrompt + Clone> SimplePrompt for LanguageModelWithBackOff<P> {
    async fn prompt(&self, prompt: Prompt) -> Result<String, LanguageModelError> {
        let strategy = self.strategy();

        let op = || {
            let prompt = prompt.clone();
            async {
                self.inner.prompt(prompt).await.map_err(|e| match e {
                    LanguageModelError::ContextLengthExceeded(e) => {
                        backoff::Error::Permanent(LanguageModelError::ContextLengthExceeded(e))
                    }
                    LanguageModelError::PermanentError(e) => {
                        backoff::Error::Permanent(LanguageModelError::PermanentError(e))
                    }
                    LanguageModelError::TransientError(e) => {
                        backoff::Error::transient(LanguageModelError::TransientError(e))
                    }
                })
            }
        };

        backoff::future::retry(strategy, op).await
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
}

#[async_trait]
impl<P: EmbeddingModel + Clone> EmbeddingModel for LanguageModelWithBackOff<P> {
    async fn embed(&self, input: Vec<String>) -> Result<Embeddings, LanguageModelError> {
        self.inner.embed(input).await
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    struct MockSimplePrompt {
        call_count: Arc<AtomicUsize>,
        should_fail_count: usize,
        error_type: MockErrorType,
    }

    #[derive(Debug, Clone, Copy)]
    enum MockErrorType {
        Transient,
        Permanent,
        ContextLengthExceeded,
    }

    #[async_trait]
    impl SimplePrompt for MockSimplePrompt {
        async fn prompt(&self, _prompt: Prompt) -> Result<String, LanguageModelError> {
            let count = self.call_count.fetch_add(1, Ordering::SeqCst);

            if count < self.should_fail_count {
                match self.error_type {
                    MockErrorType::Transient => Err(LanguageModelError::TransientError(Box::new(
                        std::io::Error::new(std::io::ErrorKind::ConnectionReset, "Transient error"),
                    ))),
                    MockErrorType::Permanent => Err(LanguageModelError::PermanentError(Box::new(
                        std::io::Error::new(std::io::ErrorKind::InvalidData, "Permanent error"),
                    ))),
                    MockErrorType::ContextLengthExceeded => Err(
                        LanguageModelError::ContextLengthExceeded(Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "Context length exceeded",
                        ))),
                    ),
                }
            } else {
                Ok("Success response".to_string())
            }
        }

        fn name(&self) -> &'static str {
            "MockSimplePrompt"
        }
    }

    #[tokio::test]
    async fn test_language_model_with_backoff_retries_transient_errors() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let mock_prompt = MockSimplePrompt {
            call_count: call_count.clone(),
            should_fail_count: 2, // Fail twice, succeed on third attempt
            error_type: MockErrorType::Transient,
        };

        let config = BackoffConfiguration {
            initial_interval_sec: 1,
            max_elapsed_time_sec: 10,
            multiplier: 1.5,
            randomization_factor: 0.5,
        };

        let model_with_backoff = LanguageModelWithBackOff::new(mock_prompt, config);

        let result = model_with_backoff.prompt(Prompt::from("Test prompt")).await;

        assert!(result.is_ok());
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
        assert_eq!(result.unwrap(), "Success response");
    }

    #[tokio::test]
    async fn test_language_model_with_backoff_does_not_retry_permanent_errors() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let mock_prompt = MockSimplePrompt {
            call_count: call_count.clone(),
            should_fail_count: 1,
            error_type: MockErrorType::Permanent,
        };

        let config = BackoffConfiguration {
            initial_interval_sec: 1,
            max_elapsed_time_sec: 10,
            multiplier: 1.5,
            randomization_factor: 0.5,
        };

        let model_with_backoff = LanguageModelWithBackOff::new(mock_prompt, config);

        let result = model_with_backoff.prompt(Prompt::from("Test prompt")).await;

        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        match result {
            Err(LanguageModelError::PermanentError(_)) => {} // Expected
            _ => panic!("Expected PermanentError"),
        }
    }

    #[tokio::test]
    async fn test_language_model_with_backoff_does_not_retry_context_length_errors() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let mock_prompt = MockSimplePrompt {
            call_count: call_count.clone(),
            should_fail_count: 1,
            error_type: MockErrorType::ContextLengthExceeded,
        };

        let config = BackoffConfiguration {
            initial_interval_sec: 1,
            max_elapsed_time_sec: 10,
            multiplier: 1.5,
            randomization_factor: 0.5,
        };

        let model_with_backoff = LanguageModelWithBackOff::new(mock_prompt, config);

        let result = model_with_backoff.prompt(Prompt::from("Test prompt")).await;

        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        match result {
            Err(LanguageModelError::ContextLengthExceeded(_)) => {} // Expected
            _ => panic!("Expected ContextLengthExceeded"),
        }
    }
}
