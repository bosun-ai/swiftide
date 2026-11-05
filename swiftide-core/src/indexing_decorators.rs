use std::fmt::{self, Debug};

use crate::chat_completion::{ChatCompletionRequest, ChatCompletionResponse};
use crate::stream_backoff::{StreamBackoff, TokioSleeper};
use crate::{ChatCompletion, ChatCompletionStream};
use crate::{EmbeddingModel, Embeddings, SimplePrompt, prompt::Prompt};

use crate::chat_completion::errors::LanguageModelError;
use anyhow::Result;
use async_trait::async_trait;
use futures_util::{StreamExt as _, TryStreamExt as _};
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

    /// Runs `operation` until it succeeds, fails permanently, or the backoff schedule is
    /// exhausted.
    ///
    /// Every failed attempt that will be retried is logged at WARN with its full cause, so a
    /// misconfiguration (e.g. a wrong base URL) is visible on the first attempt. The final failure
    /// is logged once at ERROR.
    async fn retry<T, Fut>(
        &self,
        mut operation: impl FnMut() -> Fut,
    ) -> Result<T, LanguageModelError>
    where
        Fut: Future<Output = Result<T, LanguageModelError>>,
    {
        let mut failed_attempts: usize = 0;

        let result = backoff::future::retry_notify(
            self.strategy(),
            || {
                let attempt = operation();
                async move { attempt.await.map_err(into_backoff_error) }
            },
            |error: LanguageModelError, retry_in: Duration| {
                failed_attempts += 1;
                tracing::warn!(
                    attempt = failed_attempts,
                    ?retry_in,
                    error = %ErrorWithCauses(&error),
                    "Language model request failed, retrying"
                );
            },
        )
        .await;

        result.inspect_err(|error| {
            tracing::error!(
                attempts = failed_attempts + 1,
                error = %ErrorWithCauses(error),
                "Language model request failed, giving up"
            );
        })
    }
}

/// Only transient errors are worth retrying; everything else fails immediately.
fn into_backoff_error(error: LanguageModelError) -> backoff::Error<LanguageModelError> {
    match error {
        LanguageModelError::TransientError(_) => backoff::Error::transient(error),
        LanguageModelError::ContextLengthExceeded(_) | LanguageModelError::PermanentError(_) => {
            backoff::Error::Permanent(error)
        }
    }
}

/// Displays a [`LanguageModelError`] followed by its chain of causes.
///
/// Transport errors often hide the actionable part in their source (e.g. `error sending request`
/// caused by `Connection refused`).
struct ErrorWithCauses<'a>(&'a LanguageModelError);

impl fmt::Display for ErrorWithCauses<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (LanguageModelError::ContextLengthExceeded(inner)
        | LanguageModelError::PermanentError(inner)
        | LanguageModelError::TransientError(inner)) = self.0;

        let mut message = self.0.to_string();
        let mut cause = inner.source();
        while let Some(error) = cause {
            let description = error.to_string();
            // Some errors already include their source in their own message.
            if !message.contains(&description) {
                message.push_str(": ");
                message.push_str(&description);
            }
            cause = error.source();
        }

        f.write_str(&message)
    }
}

#[async_trait]
impl<P: SimplePrompt + Clone> SimplePrompt for LanguageModelWithBackOff<P> {
    async fn prompt(&self, prompt: Prompt) -> Result<String, LanguageModelError> {
        self.retry(|| self.inner.prompt(prompt.clone())).await
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

#[async_trait]
impl<LLM: ChatCompletion + Clone> ChatCompletion for LanguageModelWithBackOff<LLM> {
    async fn complete(
        &self,
        request: &ChatCompletionRequest<'_>,
    ) -> Result<ChatCompletionResponse, LanguageModelError> {
        self.retry(|| self.inner.complete(request)).await
    }

    async fn complete_stream(&self, request: &ChatCompletionRequest<'_>) -> ChatCompletionStream {
        let strategy = self.strategy();

        let stream = self.inner.complete_stream(request).await;
        let stream = stream.map_err(into_backoff_error).boxed();
        StreamBackoff::new(stream, strategy, TokioSleeper)
            .map_err(|e| match e {
                backoff::Error::Permanent(e) => e,
                backoff::Error::Transient { err, .. } => err,
            })
            .boxed()
    }
}
#[cfg(test)]
mod tests {

    use uuid::Uuid;

    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

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

    #[derive(Clone)]
    struct MockChatCompletion {
        call_count: Arc<AtomicUsize>,
        should_fail_count: usize,
        error_type: MockErrorType,
    }

    #[async_trait]
    impl ChatCompletion for MockChatCompletion {
        async fn complete(
            &self,
            _request: &ChatCompletionRequest<'_>,
        ) -> Result<ChatCompletionResponse, LanguageModelError> {
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
                Ok(ChatCompletionResponse {
                    id: Uuid::new_v4(),
                    message: Some("Success response".to_string()),
                    tool_calls: None,
                    delta: None,
                    usage: None,
                    reasoning: None,
                })
            }
        }
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

    #[tokio::test]
    async fn test_language_model_with_backoff_retries_chat_completion_transient_errors() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let mock_chat = MockChatCompletion {
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

        let model_with_backoff = LanguageModelWithBackOff::new(mock_chat, config);

        let request: ChatCompletionRequest<'static> = Vec::new().into();

        let result = model_with_backoff.complete(&request).await;

        assert!(result.is_ok());
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
        assert_eq!(
            result.unwrap().message,
            Some("Success response".to_string())
        );
    }

    #[tokio::test]
    async fn test_language_model_with_backoff_does_not_retry_chat_completion_permanent_errors() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let mock_chat = MockChatCompletion {
            call_count: call_count.clone(),
            should_fail_count: 2, // Would fail twice if retried
            error_type: MockErrorType::Permanent,
        };

        let config = BackoffConfiguration {
            initial_interval_sec: 1,
            max_elapsed_time_sec: 10,
            multiplier: 1.5,
            randomization_factor: 0.5,
        };

        let model_with_backoff = LanguageModelWithBackOff::new(mock_chat, config);

        let request: ChatCompletionRequest<'static> = Vec::new().into();

        let result = model_with_backoff.complete(&request).await;

        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 1); // Should only be called once

        match result {
            Err(LanguageModelError::PermanentError(_)) => {} // Expected
            _ => panic!("Expected PermanentError, got {result:?}"),
        }
    }

    #[tokio::test]
    async fn test_language_model_with_backoff_does_not_retry_chat_completion_context_length_errors()
    {
        let call_count = Arc::new(AtomicUsize::new(0));
        let mock_chat = MockChatCompletion {
            call_count: call_count.clone(),
            should_fail_count: 2, // Would fail twice if retried
            error_type: MockErrorType::ContextLengthExceeded,
        };

        let config = BackoffConfiguration {
            initial_interval_sec: 1,
            max_elapsed_time_sec: 10,
            multiplier: 1.5,
            randomization_factor: 0.5,
        };

        let model_with_backoff = LanguageModelWithBackOff::new(mock_chat, config);

        let request: ChatCompletionRequest<'static> = Vec::new().into();

        let result = model_with_backoff.complete(&request).await;

        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 1); // Should only be called once

        match result {
            Err(LanguageModelError::ContextLengthExceeded(_)) => {} // Expected
            _ => panic!("Expected ContextLengthExceeded, got {result:?}"),
        }
    }

    /// Level and fields of every event emitted by this crate on the current thread.
    #[derive(Clone, Default)]
    struct CapturedEvents(Arc<std::sync::Mutex<Vec<(tracing::Level, String)>>>);

    thread_local! {
        static CURRENT_CAPTURE: std::cell::RefCell<Option<CapturedEvents>> =
            const { std::cell::RefCell::new(None) };
    }

    impl CapturedEvents {
        /// Starts capturing events emitted on the current thread.
        ///
        /// A global subscriber routing to a thread local capture is used instead of scoped
        /// subscribers, because tracing caches callsite interest across threads, which makes
        /// scoped subscribers miss events when tests run in parallel.
        fn start() -> Self {
            static INSTALL: std::sync::Once = std::sync::Once::new();
            INSTALL.call_once(|| {
                tracing::subscriber::set_global_default(ThreadLocalCapture)
                    .expect("no other global subscriber in swiftide-core tests");
            });

            let events = Self::default();
            CURRENT_CAPTURE.with(|capture| *capture.borrow_mut() = Some(events.clone()));
            events
        }

        fn at(&self, level: tracing::Level) -> Vec<String> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .filter(|(event_level, _)| *event_level == level)
                .map(|(_, fields)| fields.clone())
                .collect()
        }
    }

    struct FieldsAsText(String);

    impl tracing::field::Visit for FieldsAsText {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn Debug) {
            use std::fmt::Write as _;
            let _ = write!(self.0, "{}={value:?} ", field.name());
        }
    }

    struct ThreadLocalCapture;

    impl tracing::Subscriber for ThreadLocalCapture {
        fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
            true
        }

        fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }

        fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}

        fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

        fn event(&self, event: &tracing::Event<'_>) {
            if !event.metadata().target().starts_with("swiftide_core") {
                return;
            }
            CURRENT_CAPTURE.with(|capture| {
                if let Some(events) = capture.borrow().as_ref() {
                    let mut fields = FieldsAsText(String::new());
                    event.record(&mut fields);
                    events
                        .0
                        .lock()
                        .unwrap()
                        .push((*event.metadata().level(), fields.0));
                }
            });
        }

        fn enter(&self, _span: &tracing::span::Id) {}

        fn exit(&self, _span: &tracing::span::Id) {}
    }

    #[tokio::test(start_paused = true)]
    async fn test_retried_attempts_warn_and_eventual_success_logs_no_error() {
        let events = CapturedEvents::start();

        let call_count = Arc::new(AtomicUsize::new(0));
        let mock_chat = MockChatCompletion {
            call_count: call_count.clone(),
            should_fail_count: 2,
            error_type: MockErrorType::Transient,
        };
        let model_with_backoff =
            LanguageModelWithBackOff::new(mock_chat, BackoffConfiguration::default());

        let request: ChatCompletionRequest<'static> = Vec::new().into();
        let result = model_with_backoff.complete(&request).await;

        assert!(result.is_ok());
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
        assert!(events.at(tracing::Level::ERROR).is_empty());

        let warnings = events.at(tracing::Level::WARN);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings[0].contains("attempt=1"), "{warnings:?}");
        assert!(warnings[1].contains("attempt=2"), "{warnings:?}");
        assert!(warnings[0].contains("Transient error"), "{warnings:?}");
    }

    #[tokio::test]
    async fn test_giving_up_logs_exactly_one_error() {
        let events = CapturedEvents::start();

        let call_count = Arc::new(AtomicUsize::new(0));
        let mock_prompt = MockSimplePrompt {
            call_count: call_count.clone(),
            should_fail_count: usize::MAX,
            error_type: MockErrorType::Transient,
        };
        // One retry after a second; a second retry would exceed the elapsed time budget.
        let config = BackoffConfiguration {
            initial_interval_sec: 1,
            multiplier: 1.0,
            randomization_factor: 0.0,
            max_elapsed_time_sec: 2,
        };
        let model_with_backoff = LanguageModelWithBackOff::new(mock_prompt, config);

        let result = model_with_backoff.prompt(Prompt::from("Test prompt")).await;

        assert!(matches!(result, Err(LanguageModelError::TransientError(_))));
        let attempts = call_count.load(Ordering::SeqCst);
        assert!(attempts >= 2, "expected at least one retry, got {attempts}");

        let errors = events.at(tracing::Level::ERROR);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(
            errors[0].contains(&format!("attempts={attempts}")),
            "{errors:?}"
        );
        assert_eq!(events.at(tracing::Level::WARN).len(), attempts - 1);
    }

    #[tokio::test]
    async fn test_permanent_error_logs_one_error_and_no_retry_warning() {
        let events = CapturedEvents::start();

        let mock_prompt = MockSimplePrompt {
            call_count: Arc::new(AtomicUsize::new(0)),
            should_fail_count: 1,
            error_type: MockErrorType::Permanent,
        };
        let model_with_backoff =
            LanguageModelWithBackOff::new(mock_prompt, BackoffConfiguration::default());

        let result = model_with_backoff.prompt(Prompt::from("Test prompt")).await;

        assert!(matches!(result, Err(LanguageModelError::PermanentError(_))));
        assert_eq!(events.at(tracing::Level::ERROR).len(), 1);
        assert!(events.at(tracing::Level::WARN).is_empty());
    }

    #[test]
    fn test_error_with_causes_includes_the_underlying_cause() {
        #[derive(Debug)]
        struct RequestFailed(std::io::Error);

        impl fmt::Display for RequestFailed {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("error sending request for url (http://localhost:1/v1/chat)")
            }
        }

        impl std::error::Error for RequestFailed {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }

        let error = LanguageModelError::transient(RequestFailed(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "Connection refused",
        )));

        assert_eq!(
            ErrorWithCauses(&error).to_string(),
            "Transient error: error sending request for url (http://localhost:1/v1/chat): \
             Connection refused"
        );
    }
}
