//! This module provides integration with `OpenAI`'s API, enabling the use of language models and
//! embeddings within the Swiftide project. It includes the `OpenAI` struct for managing API clients
//! and default options for embedding and prompt models. The module is conditionally compiled based
//! on the "openai" feature flag.

use async_openai::error::OpenAIError;
use async_openai::types::CreateChatCompletionRequestArgs;
use async_openai::types::CreateEmbeddingRequestArgs;
use async_openai::types::ReasoningEffort;
use derive_builder::Builder;
use std::pin::Pin;
use std::sync::Arc;
use swiftide_core::chat_completion::Usage;
use swiftide_core::chat_completion::errors::LanguageModelError;

mod chat_completion;
mod embed;
mod responses_api;
mod simple_prompt;
mod structured_prompt;

// expose type aliases to simplify downstream use of the open ai builder invocations
pub use async_openai::config::AzureConfig;
pub use async_openai::config::OpenAIConfig;

#[cfg(feature = "tiktoken")]
use crate::tiktoken::TikToken;
#[cfg(feature = "tiktoken")]
use anyhow::Result;
#[cfg(feature = "tiktoken")]
use swiftide_core::Estimatable;
#[cfg(feature = "tiktoken")]
use swiftide_core::EstimateTokens;

/// The `OpenAI` struct encapsulates an `OpenAI` client and default options for embedding and prompt
/// models. It uses the `Builder` pattern for flexible and customizable instantiation.
///
/// # Example
///
/// ```no_run
/// # use swiftide_integrations::openai::{OpenAI, Options};
/// # use swiftide_integrations::openai::OpenAIConfig;
///
/// // Create an OpenAI client with default options. The client will use the OPENAI_API_KEY environment variable.
/// let openai = OpenAI::builder()
///     .default_embed_model("text-embedding-3-small")
///     .default_prompt_model("gpt-4")
///     .build().unwrap();
///
/// // Create an OpenAI client with a custom api key.
/// let openai = OpenAI::builder()
///     .default_embed_model("text-embedding-3-small")
///     .default_prompt_model("gpt-4")
///     .client(async_openai::Client::with_config(async_openai::config::OpenAIConfig::default().with_api_key("my-api-key")))
///     .build().unwrap();
///
/// // Create an OpenAI client with custom options
/// let openai = OpenAI::builder()
///     .default_embed_model("text-embedding-3-small")
///     .default_prompt_model("gpt-4")
///     .default_options(
///         Options::builder()
///           .temperature(1.0)
///           .parallel_tool_calls(false)
///           .user("MyUserId")
///     )
///     .build().unwrap();
/// ```
pub type OpenAI = GenericOpenAI<OpenAIConfig>;
pub type OpenAIBuilder = GenericOpenAIBuilder<OpenAIConfig>;

#[derive(Builder, Clone)]
#[builder(setter(into, strip_option))]
/// Generic client for `OpenAI` APIs.
pub struct GenericOpenAI<
    C: async_openai::config::Config + Default = async_openai::config::OpenAIConfig,
> {
    /// The `OpenAI` client, wrapped in an `Arc` for thread-safe reference counting.
    /// Defaults to a new instance of `async_openai::Client`.
    #[builder(
        default = "Arc::new(async_openai::Client::<C>::default())",
        setter(custom)
    )]
    client: Arc<async_openai::Client<C>>,

    /// Default options for embedding and prompt models.
    #[builder(default, setter(custom))]
    pub(crate) default_options: Options,

    #[cfg(feature = "tiktoken")]
    #[cfg_attr(feature = "tiktoken", builder(default))]
    pub(crate) tiktoken: TikToken,

    /// Convenience option to stream the full response. Defaults to true, because nobody has time
    /// to reconstruct the delta. Disabling this will make the streamed content only return the
    /// delta, for when performance matters. This only has effect when streaming is enabled.
    #[builder(default = true)]
    pub stream_full: bool,

    #[cfg(feature = "metrics")]
    #[builder(default)]
    /// Optional metadata to attach to metrics emitted by this client.
    metric_metadata: Option<std::collections::HashMap<String, String>>,

    /// Opt-in flag to use OpenAI's Responses API instead of the legacy Chat Completions API.
    #[builder(default)]
    pub(crate) use_responses_api: bool,

    /// A callback function that is called when usage information is available.
    #[builder(default, setter(custom))]
    #[allow(clippy::type_complexity)]
    on_usage: Option<
        Arc<
            dyn for<'a> Fn(
                    &'a Usage,
                ) -> Pin<
                    Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>,
                > + Send
                + Sync,
        >,
    >,
}

impl<C: async_openai::config::Config + Default + std::fmt::Debug> std::fmt::Debug
    for GenericOpenAI<C>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GenericOpenAI")
            .field("client", &self.client)
            .field("default_options", &self.default_options)
            .field("stream_full", &self.stream_full)
            .field("use_responses_api", &self.use_responses_api)
            .finish_non_exhaustive()
    }
}

/// The `Options` struct holds configuration options for the `OpenAI` client.
/// It includes optional fields for specifying the embedding and prompt models.
#[derive(Debug, Clone, Builder, Default)]
#[builder(setter(strip_option))]
pub struct Options {
    /// The default embedding model to use, if specified.
    #[builder(default, setter(into))]
    pub embed_model: Option<String>,
    /// The default prompt model to use, if specified.
    #[builder(default, setter(into))]
    pub prompt_model: Option<String>,

    #[builder(default)]
    /// Option to enable or disable parallel tool calls for completions.
    ///
    /// At this moment, o1 and o3-mini do not support it and should be set to `None`.
    pub parallel_tool_calls: Option<bool>,

    /// Maximum number of tokens to generate in the completion.
    ///
    /// By default, the limit is disabled
    #[builder(default)]
    pub max_completion_tokens: Option<u32>,

    /// Temperature setting for the model.
    #[builder(default)]
    pub temperature: Option<f32>,

    /// Reasoning effor for reasoning models.
    #[builder(default, setter(into))]
    pub reasoning_effort: Option<ReasoningEffort>,

    /// This feature is in Beta. If specified, our system will make a best effort to sample
    /// deterministically, such that repeated requests with the same seed and parameters should
    /// return the same result. Determinism is not guaranteed, and you should refer to the
    /// `system_fingerprint` response parameter to monitor changes in the backend.
    #[builder(default)]
    pub seed: Option<i64>,

    /// Number between -2.0 and 2.0. Positive values penalize new tokens based on whether they
    /// appear in the text so far, increasing the model’s likelihood to talk about new topics.
    #[builder(default)]
    pub presence_penalty: Option<f32>,

    /// Developer-defined tags and values used for filtering completions in the dashboard.
    #[builder(default, setter(into))]
    pub metadata: Option<serde_json::Value>,

    /// A unique identifier representing your end-user, which can help `OpenAI` to monitor and
    /// detect abuse.
    #[builder(default, setter(into))]
    pub user: Option<String>,

    #[builder(default)]
    /// The number of dimensions the resulting output embeddings should have. Only supported in
    /// text-embedding-3 and later models.
    pub dimensions: Option<u32>,
}

impl Options {
    /// Creates a new `OptionsBuilder` for constructing `Options` instances.
    pub fn builder() -> OptionsBuilder {
        OptionsBuilder::default()
    }

    /// Extends options with other options
    pub fn merge(&mut self, other: &Options) {
        if let Some(embed_model) = &other.embed_model {
            self.embed_model = Some(embed_model.clone());
        }
        if let Some(prompt_model) = &other.prompt_model {
            self.prompt_model = Some(prompt_model.clone());
        }
        if let Some(parallel_tool_calls) = other.parallel_tool_calls {
            self.parallel_tool_calls = Some(parallel_tool_calls);
        }
        if let Some(max_completion_tokens) = other.max_completion_tokens {
            self.max_completion_tokens = Some(max_completion_tokens);
        }
        if let Some(temperature) = other.temperature {
            self.temperature = Some(temperature);
        }
        if let Some(reasoning_effort) = &other.reasoning_effort {
            self.reasoning_effort = Some(reasoning_effort.clone());
        }
        if let Some(seed) = other.seed {
            self.seed = Some(seed);
        }
        if let Some(presence_penalty) = other.presence_penalty {
            self.presence_penalty = Some(presence_penalty);
        }
        if let Some(metadata) = &other.metadata {
            self.metadata = Some(metadata.clone());
        }
        if let Some(user) = &other.user {
            self.user = Some(user.clone());
        }
    }
}

impl From<OptionsBuilder> for Options {
    fn from(value: OptionsBuilder) -> Self {
        Self {
            embed_model: value.embed_model.flatten(),
            prompt_model: value.prompt_model.flatten(),
            parallel_tool_calls: value.parallel_tool_calls.flatten(),
            max_completion_tokens: value.max_completion_tokens.flatten(),
            temperature: value.temperature.flatten(),
            reasoning_effort: value.reasoning_effort.flatten(),
            presence_penalty: value.presence_penalty.flatten(),
            seed: value.seed.flatten(),
            metadata: value.metadata.flatten(),
            user: value.user.flatten(),
            dimensions: value.dimensions.flatten(),
        }
    }
}

impl From<&mut OptionsBuilder> for Options {
    fn from(value: &mut OptionsBuilder) -> Self {
        let value = value.clone();
        Self {
            embed_model: value.embed_model.flatten(),
            prompt_model: value.prompt_model.flatten(),
            parallel_tool_calls: value.parallel_tool_calls.flatten(),
            max_completion_tokens: value.max_completion_tokens.flatten(),
            temperature: value.temperature.flatten(),
            reasoning_effort: value.reasoning_effort.flatten(),
            presence_penalty: value.presence_penalty.flatten(),
            seed: value.seed.flatten(),
            metadata: value.metadata.flatten(),
            user: value.user.flatten(),
            dimensions: value.dimensions.flatten(),
        }
    }
}

impl OpenAI {
    /// Creates a new `OpenAIBuilder` for constructing `OpenAI` instances.
    pub fn builder() -> OpenAIBuilder {
        OpenAIBuilder::default()
    }
}

impl<C: async_openai::config::Config + Default + Sync + Send + std::fmt::Debug>
    GenericOpenAIBuilder<C>
{
    /// Adds a callback function that will be called when usage information is available.
    pub fn on_usage<F>(&mut self, func: F) -> &mut Self
    where
        F: Fn(&Usage) -> anyhow::Result<()> + Send + Sync + 'static,
    {
        let func = Arc::new(func);
        self.on_usage = Some(Some(Arc::new(move |usage: &Usage| {
            let func = func.clone();
            Box::pin(async move { func(usage) })
        })));

        self
    }

    /// Adds an asynchronous callback function that will be called when usage information is
    /// available.
    pub fn on_usage_async<F>(&mut self, func: F) -> &mut Self
    where
        F: for<'a> Fn(
                &'a Usage,
            )
                -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>>
            + Send
            + Sync
            + 'static,
    {
        let func = Arc::new(func);
        self.on_usage = Some(Some(Arc::new(move |usage: &Usage| {
            let func = func.clone();
            Box::pin(async move { func(usage).await })
        })));

        self
    }
    /// Sets the `OpenAI` client for the `OpenAI` instance.
    ///
    /// # Parameters
    /// - `client`: The `OpenAI` client to set.
    ///
    /// # Returns
    /// A mutable reference to the `OpenAIBuilder`.
    pub fn client(&mut self, client: async_openai::Client<C>) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }

    /// Sets the default embedding model for the `OpenAI` instance.
    ///
    /// # Parameters
    /// - `model`: The embedding model to set.
    ///
    /// # Returns
    /// A mutable reference to the `OpenAIBuilder`.
    pub fn default_embed_model(&mut self, model: impl Into<String>) -> &mut Self {
        if let Some(options) = self.default_options.as_mut() {
            options.embed_model = Some(model.into());
        } else {
            self.default_options = Some(Options {
                embed_model: Some(model.into()),
                ..Default::default()
            });
        }
        self
    }

    /// Sets the `user` field used by `OpenAI` to monitor and detect usage and abuse.
    pub fn for_end_user(&mut self, user: impl Into<String>) -> &mut Self {
        if let Some(options) = self.default_options.as_mut() {
            options.user = Some(user.into());
        } else {
            self.default_options = Some(Options {
                user: Some(user.into()),
                ..Default::default()
            });
        }
        self
    }

    /// Enable or disable parallel tool calls for completions.
    ///
    /// Note that currently reasoning models do not support parallel tool calls
    ///
    /// Defaults to `true`
    pub fn parallel_tool_calls(&mut self, parallel_tool_calls: Option<bool>) -> &mut Self {
        if let Some(options) = self.default_options.as_mut() {
            options.parallel_tool_calls = parallel_tool_calls;
        } else {
            self.default_options = Some(Options {
                parallel_tool_calls,
                ..Default::default()
            });
        }
        self
    }

    /// Sets the default prompt model for the `OpenAI` instance.
    ///
    /// # Parameters
    /// - `model`: The prompt model to set.
    ///
    /// # Returns
    /// A mutable reference to the `OpenAIBuilder`.
    pub fn default_prompt_model(&mut self, model: impl Into<String>) -> &mut Self {
        if let Some(options) = self.default_options.as_mut() {
            options.prompt_model = Some(model.into());
        } else {
            self.default_options = Some(Options {
                prompt_model: Some(model.into()),
                ..Default::default()
            });
        }
        self
    }

    /// Sets the default options to use for requests to the `OpenAI` API.
    ///
    /// Merges with any existing options
    pub fn default_options(&mut self, options: impl Into<Options>) -> &mut Self {
        if let Some(existing_options) = self.default_options.as_mut() {
            existing_options.merge(&options.into());
        } else {
            self.default_options = Some(options.into());
        }
        self
    }
}

impl<C: async_openai::config::Config + Default> GenericOpenAI<C> {
    /// Estimates the number of tokens for implementors of the `Estimatable` trait.
    ///
    /// I.e. `String`, `ChatMessage` etc
    ///
    /// # Errors
    ///
    /// Errors if tokinization fails in any way
    #[cfg(feature = "tiktoken")]
    pub async fn estimate_tokens(&self, value: impl Estimatable) -> Result<usize> {
        self.tiktoken.estimate(value).await
    }

    pub fn with_default_prompt_model(&mut self, model: impl Into<String>) -> &mut Self {
        self.default_options = Options {
            prompt_model: Some(model.into()),
            ..self.default_options.clone()
        };
        self
    }

    pub fn with_default_embed_model(&mut self, model: impl Into<String>) -> &mut Self {
        self.default_options = Options {
            embed_model: Some(model.into()),
            ..self.default_options.clone()
        };
        self
    }

    /// Retrieve a reference to the inner `OpenAI` client.
    pub fn client(&self) -> &Arc<async_openai::Client<C>> {
        &self.client
    }

    /// Retrieve a reference to the default options for the `OpenAI` instance.
    pub fn options(&self) -> &Options {
        &self.default_options
    }

    /// Retrieve a mutable reference to the default options for the `OpenAI` instance.
    pub fn options_mut(&mut self) -> &mut Options {
        &mut self.default_options
    }

    /// Returns whether the Responses API is enabled for this client.
    pub fn is_responses_api_enabled(&self) -> bool {
        self.use_responses_api
    }

    fn chat_completion_request_defaults(&self) -> CreateChatCompletionRequestArgs {
        let mut args = CreateChatCompletionRequestArgs::default();

        let options = &self.default_options;

        if let Some(parallel_tool_calls) = options.parallel_tool_calls {
            args.parallel_tool_calls(parallel_tool_calls);
        }

        if let Some(max_tokens) = options.max_completion_tokens {
            args.max_completion_tokens(max_tokens);
        }

        if let Some(temperature) = options.temperature {
            args.temperature(temperature);
        }

        if let Some(reasoning_effort) = &options.reasoning_effort {
            args.reasoning_effort(reasoning_effort.clone());
        }

        if let Some(seed) = options.seed {
            args.seed(seed);
        }

        if let Some(presence_penalty) = options.presence_penalty {
            args.presence_penalty(presence_penalty);
        }

        if let Some(metadata) = &options.metadata {
            args.metadata(metadata.clone());
        }

        if let Some(user) = &options.user {
            args.user(user.clone());
        }

        args
    }

    fn embed_request_defaults(&self) -> CreateEmbeddingRequestArgs {
        let mut args = CreateEmbeddingRequestArgs::default();

        let options = &self.default_options;

        if let Some(user) = &options.user {
            args.user(user.clone());
        }

        if let Some(dimensions) = options.dimensions {
            args.dimensions(dimensions);
        }

        args
    }
}

pub fn openai_error_to_language_model_error(e: OpenAIError) -> LanguageModelError {
    match e {
        OpenAIError::ApiError(api_error) => {
            // If the response is an ApiError, it could be a context length exceeded error
            if api_error.code == Some("context_length_exceeded".to_string()) {
                LanguageModelError::context_length_exceeded(OpenAIError::ApiError(api_error))
            } else {
                LanguageModelError::permanent(OpenAIError::ApiError(api_error))
            }
        }
        OpenAIError::Reqwest(e) => {
            // async_openai passes any network errors as reqwest errors, so we just assume they are
            // recoverable
            LanguageModelError::transient(e)
        }
        OpenAIError::JSONDeserialize(_) => {
            // OpenAI generated a non-json response, probably a temporary problem on their side
            // (i.e. reverse proxy can't find an available backend)
            LanguageModelError::transient(e)
        }
        OpenAIError::StreamError(e) => {
            // Note that this will _retry_ the stream. We have to assume that the stream just
            // started if a 429 happens. For future readers, internally clients streaming crate
            // (eventsource), has a backoff mechanism as well
            if e.contains("Too Many Requests") {
                LanguageModelError::transient(e)
            } else {
                LanguageModelError::permanent(e)
            }
        }
        OpenAIError::FileSaveError(_)
        | OpenAIError::FileReadError(_)
        | OpenAIError::InvalidArgument(_) => LanguageModelError::permanent(e),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use async_openai::error::{ApiError, OpenAIError};

    /// test default embed model
    #[test]
    fn test_default_embed_and_prompt_model() {
        let openai: OpenAI = OpenAI::builder()
            .default_embed_model("gpt-3")
            .default_prompt_model("gpt-4")
            .build()
            .unwrap();
        assert_eq!(
            openai.default_options.embed_model,
            Some("gpt-3".to_string())
        );
        assert_eq!(
            openai.default_options.prompt_model,
            Some("gpt-4".to_string())
        );

        let openai: OpenAI = OpenAI::builder()
            .default_prompt_model("gpt-4")
            .default_embed_model("gpt-3")
            .build()
            .unwrap();
        assert_eq!(
            openai.default_options.prompt_model,
            Some("gpt-4".to_string())
        );
        assert_eq!(
            openai.default_options.embed_model,
            Some("gpt-3".to_string())
        );
    }

    #[test]
    fn test_use_responses_api_flag() {
        let openai: OpenAI = OpenAI::builder().use_responses_api(true).build().unwrap();

        assert!(openai.is_responses_api_enabled());
    }

    #[test]
    fn test_context_length_exceeded_error() {
        // Create an API error with the context_length_exceeded code
        let api_error = ApiError {
            message: "This model's maximum context length is 8192 tokens".to_string(),
            r#type: Some("invalid_request_error".to_string()),
            param: Some("messages".to_string()),
            code: Some("context_length_exceeded".to_string()),
        };

        let openai_error = OpenAIError::ApiError(api_error);
        let result = openai_error_to_language_model_error(openai_error);

        // Verify it's categorized as ContextLengthExceeded
        match result {
            LanguageModelError::ContextLengthExceeded(_) => {} // Expected
            _ => panic!("Expected ContextLengthExceeded error, got {result:?}"),
        }
    }

    #[test]
    fn test_api_error_permanent() {
        // Create a generic API error (not context length exceeded)
        let api_error = ApiError {
            message: "Invalid API key".to_string(),
            r#type: Some("invalid_request_error".to_string()),
            param: Some("api_key".to_string()),
            code: Some("invalid_api_key".to_string()),
        };

        let openai_error = OpenAIError::ApiError(api_error);
        let result = openai_error_to_language_model_error(openai_error);

        // Verify it's categorized as PermanentError
        match result {
            LanguageModelError::PermanentError(_) => {} // Expected
            _ => panic!("Expected PermanentError, got {result:?}"),
        }
    }

    #[test]
    fn test_file_save_error_is_permanent() {
        // Create a file save error
        let openai_error = OpenAIError::FileSaveError("Failed to save file".to_string());
        let result = openai_error_to_language_model_error(openai_error);

        // Verify it's categorized as PermanentError
        match result {
            LanguageModelError::PermanentError(_) => {} // Expected
            _ => panic!("Expected PermanentError, got {result:?}"),
        }
    }

    #[test]
    fn test_file_read_error_is_permanent() {
        // Create a file read error
        let openai_error = OpenAIError::FileReadError("Failed to read file".to_string());
        let result = openai_error_to_language_model_error(openai_error);

        // Verify it's categorized as PermanentError
        match result {
            LanguageModelError::PermanentError(_) => {} // Expected
            _ => panic!("Expected PermanentError, got {result:?}"),
        }
    }

    #[test]
    fn test_stream_error_is_permanent() {
        // Create a stream error
        let openai_error = OpenAIError::StreamError("Stream failed".to_string());
        let result = openai_error_to_language_model_error(openai_error);

        // Verify it's categorized as PermanentError
        match result {
            LanguageModelError::PermanentError(_) => {} // Expected
            _ => panic!("Expected PermanentError, got {result:?}"),
        }
    }

    #[test]
    fn test_invalid_argument_is_permanent() {
        // Create an invalid argument error
        let openai_error = OpenAIError::InvalidArgument("Invalid argument".to_string());
        let result = openai_error_to_language_model_error(openai_error);

        // Verify it's categorized as PermanentError
        match result {
            LanguageModelError::PermanentError(_) => {} // Expected
            _ => panic!("Expected PermanentError, got {result:?}"),
        }
    }
}
