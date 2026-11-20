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

#[cfg(feature = "circuit-breaker")]
pub use llm_resilience::{CircuitBreaker, CircuitStatus};

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
    #[cfg(feature = "circuit-breaker")]
    circuit_breaker: Option<CircuitBreaker>,
}

impl<P: Clone> LanguageModelWithBackOff<P> {
    pub fn new(client: P, config: BackoffConfiguration) -> Self {
        Self {
            inner: client,
            config,
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: None,
        }
    }

    /// Adds a circuit breaker in front of the retries.
    ///
    /// After `failure_threshold` consecutive transient failures (a success resets the count), the
    /// breaker opens and requests fail immediately with a [`LanguageModelError::PermanentError`]
    /// instead of waiting out the full backoff schedule. Once the timeout has passed, probe
    /// requests are let through, one at a time by default (the supplied breaker sets the limit,
    /// see [`CircuitBreaker::with_max_probes`]); enough successful probes close the breaker, a
    /// failed probe reopens it with a longer wait. A probe that has not finished within
    /// `max_elapsed_time_sec` counts as failed, so a stalled call cannot hold the probe slot.
    /// Permanent and context length errors are not counted, as they say nothing about the health
    /// of the provider.
    ///
    /// The breaker is shared by clones of this model (and by clones of the breaker), so every
    /// pipeline step using a clone sees the same provider health. Streaming completions are not
    /// guarded.
    ///
    /// ```
    /// # use std::time::Duration;
    /// # use swiftide_core::indexing_decorators::CircuitBreaker;
    /// // Open after 5 transient failures in a row, probe again after 30 seconds, close when
    /// // every probe succeeds.
    /// let breaker = CircuitBreaker::new(5, 1.0, Duration::from_secs(30));
    /// # let _ = breaker;
    /// ```
    #[cfg(feature = "circuit-breaker")]
    #[must_use]
    pub fn with_circuit_breaker(mut self, circuit_breaker: CircuitBreaker) -> Self {
        self.circuit_breaker = Some(circuit_breaker);
        self
    }

    /// Returns the state of the circuit breaker, if one is configured.
    #[cfg(feature = "circuit-breaker")]
    pub fn circuit_state(&self) -> Option<CircuitStatus> {
        self.circuit_breaker
            .as_ref()
            .map(CircuitBreaker::status_sync)
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
        // `backoff` creates the next attempt right after it schedules the retry sleep, so the
        // breaker is consulted inside the attempt, when it is first polled after the sleep.
        // Futures are lazy, so an attempt that is turned away never reaches the provider.
        #[cfg(feature = "circuit-breaker")]
        let mut operation = {
            let circuit_breaker = self.circuit_breaker.clone();
            let probe_deadline = Duration::from_secs(self.config.max_elapsed_time_sec);
            move || {
                let circuit_breaker = circuit_breaker.clone();
                let attempt = operation();

                async move {
                    match circuit_breaker {
                        Some(circuit_breaker) => {
                            guarded(&circuit_breaker, probe_deadline, attempt).await
                        }
                        None => attempt.await,
                    }
                }
            }
        };

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

/// Runs one attempt through the circuit breaker.
///
/// Admission and the breaker's state changes happen under one lock in `llm-resilience`: while
/// half-open only as many probes as the breaker allows (one by default) are admitted, and the
/// permit carries the breaker generation it was admitted in, so a slow request admitted before
/// the breaker opened cannot close it when it finishes. Dropping the permit (a cancelled attempt,
/// or a non-transient error) records nothing and frees a probe slot.
///
/// Only transient errors count as failures; see [`into_backoff_error`]. When this failure opens
/// the breaker, it is returned as a permanent error so the retry loop stops now instead of
/// sleeping for another backoff interval first.
///
/// A half-open probe that has not finished within `probe_deadline` is cancelled and recorded as
/// a failure, so a stalled provider call cannot hold the probe slot and keep every other request
/// turned away.
#[cfg(feature = "circuit-breaker")]
async fn guarded<T>(
    circuit_breaker: &CircuitBreaker,
    probe_deadline: Duration,
    attempt: impl Future<Output = Result<T, LanguageModelError>>,
) -> Result<T, LanguageModelError> {
    let Some(permit) = circuit_breaker.try_acquire() else {
        return Err(LanguageModelError::permanent(
            "circuit breaker is open, not sending request",
        ));
    };

    let outcome = if permit.is_probe() && !probe_deadline.is_zero() {
        tokio::time::timeout(probe_deadline, attempt)
            .await
            .unwrap_or_else(|_| {
                Err(LanguageModelError::transient(format!(
                    "circuit breaker probe did not finish within {probe_deadline:?}"
                )))
            })
    } else {
        attempt.await
    };

    match outcome {
        Ok(value) => {
            permit.success();
            Ok(value)
        }
        Err(LanguageModelError::TransientError(error)) => {
            permit.failure();
            if circuit_breaker.is_open_sync() {
                Err(LanguageModelError::permanent(CircuitOpened(error)))
            } else {
                Err(LanguageModelError::TransientError(error))
            }
        }
        // Permanent and context length errors say nothing about the provider's health
        Err(
            error @ (LanguageModelError::ContextLengthExceeded(_)
            | LanguageModelError::PermanentError(_)),
        ) => {
            drop(permit);
            Err(error)
        }
    }
}

/// A transient error after which the circuit breaker was open.
///
/// Usually this is the failure that opened it. When several clones fail at the same moment,
/// another one may have opened it first; either way the breaker is open and retrying is pointless.
#[cfg(feature = "circuit-breaker")]
#[derive(Debug)]
struct CircuitOpened(Box<dyn std::error::Error + Send + Sync>);

#[cfg(feature = "circuit-breaker")]
impl fmt::Display for CircuitOpened {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "circuit breaker opened after: {}", self.0)
    }
}

#[cfg(feature = "circuit-breaker")]
impl std::error::Error for CircuitOpened {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.0.as_ref())
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

    #[cfg(feature = "circuit-breaker")]
    #[tokio::test(start_paused = true)]
    async fn test_circuit_breaker_stops_requests_after_repeated_transient_errors() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let mock_prompt = MockSimplePrompt {
            call_count: call_count.clone(),
            should_fail_count: usize::MAX,
            error_type: MockErrorType::Transient,
        };

        let model_with_backoff =
            LanguageModelWithBackOff::new(mock_prompt, BackoffConfiguration::default())
                .with_circuit_breaker(CircuitBreaker::new(2, 1.0, Duration::from_hours(1)));
        let clone = model_with_backoff.clone();

        let result = model_with_backoff.prompt(Prompt::from("Test prompt")).await;

        assert!(matches!(result, Err(LanguageModelError::PermanentError(_))));
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
        assert_eq!(
            model_with_backoff.circuit_state(),
            Some(CircuitStatus::Open)
        );

        // Clones share the breaker and fail without calling the provider
        let result = clone.prompt(Prompt::from("Test prompt")).await;

        assert!(matches!(result, Err(LanguageModelError::PermanentError(_))));
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    #[cfg(feature = "circuit-breaker")]
    #[tokio::test]
    async fn test_circuit_breaker_ignores_permanent_errors() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let mock_prompt = MockSimplePrompt {
            call_count: call_count.clone(),
            should_fail_count: 3,
            error_type: MockErrorType::Permanent,
        };

        let model_with_backoff =
            LanguageModelWithBackOff::new(mock_prompt, BackoffConfiguration::default())
                .with_circuit_breaker(CircuitBreaker::new(1, 1.0, Duration::from_hours(1)));

        for _ in 0..3 {
            let result = model_with_backoff.prompt(Prompt::from("Test prompt")).await;
            assert!(matches!(result, Err(LanguageModelError::PermanentError(_))));
        }

        assert_eq!(call_count.load(Ordering::SeqCst), 3);
        assert_eq!(
            model_with_backoff.circuit_state(),
            Some(CircuitStatus::Closed)
        );
        assert!(
            model_with_backoff
                .prompt(Prompt::from("Test prompt"))
                .await
                .is_ok()
        );
    }

    #[cfg(feature = "circuit-breaker")]
    #[tokio::test(start_paused = true)]
    async fn test_circuit_breaker_fails_without_waiting_when_it_opens() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let mock_prompt = MockSimplePrompt {
            call_count: call_count.clone(),
            should_fail_count: usize::MAX,
            error_type: MockErrorType::Transient,
        };

        let model_with_backoff = LanguageModelWithBackOff::new(
            mock_prompt,
            BackoffConfiguration {
                initial_interval_sec: 10,
                randomization_factor: 0.0,
                ..Default::default()
            },
        )
        .with_circuit_breaker(CircuitBreaker::new(2, 1.0, Duration::from_hours(1)));

        let started = tokio::time::Instant::now();
        let result = model_with_backoff.prompt(Prompt::from("Test prompt")).await;

        // One backoff sleep between the two attempts, none after the one that opened the breaker.
        // `backoff` can round the interval up by a nanosecond, which tokio rounds to a timer tick.
        let elapsed = started.elapsed();
        assert!(
            elapsed >= Duration::from_secs(10) && elapsed < Duration::from_secs(11),
            "{elapsed:?}"
        );
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
        let Err(LanguageModelError::PermanentError(error)) = result else {
            panic!("expected a permanent error, got {result:?}");
        };
        assert!(error.to_string().contains("Transient error"));
    }

    #[cfg(feature = "circuit-breaker")]
    #[tokio::test(start_paused = true)]
    async fn test_circuit_breaker_checked_when_retry_runs_not_when_scheduled() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let mock_prompt = MockSimplePrompt {
            call_count: call_count.clone(),
            should_fail_count: usize::MAX,
            error_type: MockErrorType::Transient,
        };

        let first = LanguageModelWithBackOff::new(mock_prompt, BackoffConfiguration::default())
            .with_circuit_breaker(CircuitBreaker::new(2, 1.0, Duration::from_hours(1)));
        let second = first.clone();

        // Whichever runs first fails once and sleeps; the other one opens the breaker meanwhile.
        // The sleeping retry must then be turned away instead of calling the provider again.
        let (a, b) = tokio::join!(
            first.prompt(Prompt::from("Test prompt")),
            second.prompt(Prompt::from("Test prompt"))
        );

        assert!(matches!(a, Err(LanguageModelError::PermanentError(_))));
        assert!(matches!(b, Err(LanguageModelError::PermanentError(_))));
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
        assert_eq!(first.circuit_state(), Some(CircuitStatus::Open));
    }

    #[cfg(feature = "circuit-breaker")]
    #[derive(Debug, Clone)]
    struct SlowPrompt {
        call_count: Arc<AtomicUsize>,
    }

    #[cfg(feature = "circuit-breaker")]
    #[async_trait]
    impl SimplePrompt for SlowPrompt {
        async fn prompt(&self, _prompt: Prompt) -> Result<String, LanguageModelError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_secs(5)).await;
            Ok("Success response".to_string())
        }

        fn name(&self) -> &'static str {
            "SlowPrompt"
        }
    }

    #[cfg(feature = "circuit-breaker")]
    fn half_open_model(call_count: &Arc<AtomicUsize>) -> LanguageModelWithBackOff<SlowPrompt> {
        let model_with_backoff = LanguageModelWithBackOff::new(
            SlowPrompt {
                call_count: call_count.clone(),
            },
            BackoffConfiguration::default(),
        )
        .with_circuit_breaker(CircuitBreaker::new(1, 1.0, Duration::ZERO));

        // Trip the breaker; with a zero timeout the next request is a half-open probe
        let breaker = model_with_backoff.circuit_breaker.clone().unwrap();
        breaker.try_acquire().unwrap().failure();
        assert_eq!(breaker.status_sync(), CircuitStatus::Open);
        model_with_backoff
    }

    #[cfg(feature = "circuit-breaker")]
    #[tokio::test(start_paused = true)]
    async fn test_circuit_breaker_admits_one_probe_at_a_time_when_half_open() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let model_with_backoff = half_open_model(&call_count);
        let clone = model_with_backoff.clone();

        let (a, b) = tokio::join!(
            model_with_backoff.prompt(Prompt::from("Test prompt")),
            clone.prompt(Prompt::from("Test prompt"))
        );

        // One probe reached the provider, the concurrent request was turned away
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
        assert_eq!(
            [&a, &b].iter().filter(|result| result.is_ok()).count(),
            1,
            "{a:?} {b:?}"
        );
        assert_eq!(
            model_with_backoff.circuit_state(),
            Some(CircuitStatus::Closed)
        );

        // The probe closed the breaker, so requests flow again
        assert!(clone.prompt(Prompt::from("Test prompt")).await.is_ok());
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    #[cfg(feature = "circuit-breaker")]
    #[tokio::test(start_paused = true)]
    async fn test_circuit_breaker_releases_probe_when_cancelled() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let model_with_backoff = half_open_model(&call_count);

        // The probe is dropped before the provider answers
        let cancelled = tokio::time::timeout(
            Duration::from_secs(1),
            model_with_backoff.prompt(Prompt::from("Test prompt")),
        )
        .await;
        assert!(cancelled.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        // The slot was released, so the next request can probe
        assert!(
            model_with_backoff
                .prompt(Prompt::from("Test prompt"))
                .await
                .is_ok()
        );
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    /// Waits until the provider has been called `calls` times, failing if `task` finishes first.
    #[cfg(feature = "circuit-breaker")]
    async fn wait_for_provider_call<T>(
        call_count: &AtomicUsize,
        calls: usize,
        task: &tokio::task::JoinHandle<T>,
    ) {
        while call_count.load(Ordering::SeqCst) < calls {
            assert!(
                !task.is_finished() || call_count.load(Ordering::SeqCst) >= calls,
                "request finished before reaching the provider"
            );
            tokio::task::yield_now().await;
        }
    }

    #[cfg(feature = "circuit-breaker")]
    #[tokio::test(start_paused = true)]
    async fn test_circuit_breaker_probe_that_stalls_counts_as_failed() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let model_with_backoff = LanguageModelWithBackOff::new(
            SlowPrompt {
                call_count: call_count.clone(),
            },
            BackoffConfiguration {
                max_elapsed_time_sec: 2,
                ..Default::default()
            },
        )
        .with_circuit_breaker(CircuitBreaker::new(1, 1.0, Duration::ZERO));
        let breaker = model_with_backoff.circuit_breaker.clone().unwrap();
        breaker.try_acquire().unwrap().failure();
        assert_eq!(breaker.status_sync(), CircuitStatus::Open);

        // The probe takes 5 seconds, longer than the 2 second deadline
        let started = tokio::time::Instant::now();
        let result = model_with_backoff.prompt(Prompt::from("Test prompt")).await;

        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
        let Err(LanguageModelError::PermanentError(error)) = result else {
            panic!("expected a permanent error, got {result:?}");
        };
        assert!(error.to_string().contains("did not finish"), "{error}");
        // The failed probe reopened the breaker and freed its slot
        assert_eq!(breaker.status_sync(), CircuitStatus::Open);
        assert_eq!(breaker.probes_in_flight(), 0);
    }

    #[cfg(feature = "circuit-breaker")]
    #[tokio::test(start_paused = true)]
    async fn test_circuit_breaker_ignores_success_admitted_before_it_opened() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let breaker = CircuitBreaker::new(1, 1.0, Duration::ZERO);
        let model_with_backoff = LanguageModelWithBackOff::new(
            SlowPrompt {
                call_count: call_count.clone(),
            },
            BackoffConfiguration::default(),
        )
        .with_circuit_breaker(breaker.clone());

        // A request is admitted while the breaker is closed and takes 5 seconds
        let model = model_with_backoff.clone();
        let stale = tokio::spawn(async move { model.prompt(Prompt::from("Test prompt")).await });
        wait_for_provider_call(&call_count, 1, &stale).await;
        tokio::time::advance(Duration::from_secs(1)).await;

        // Meanwhile the breaker opens and a probe starts
        breaker.trip().await;
        let model = model_with_backoff.clone();
        let probe = tokio::spawn(async move { model.prompt(Prompt::from("Test prompt")).await });
        wait_for_provider_call(&call_count, 2, &probe).await;
        assert_eq!(breaker.status_sync(), CircuitStatus::HalfOpen);

        // The older request succeeds first; it must not close the breaker
        assert!(stale.await.unwrap().is_ok());
        assert_eq!(breaker.status_sync(), CircuitStatus::HalfOpen);

        // Only the probe decides
        assert!(probe.await.unwrap().is_ok());
        assert_eq!(breaker.status_sync(), CircuitStatus::Closed);
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
