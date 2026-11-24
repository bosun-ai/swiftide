//! This module provides integration with `OpenAI`'s API, enabling the use of language models and
//! embeddings within the Swiftide project. It includes the `OpenAI` struct for managing API clients
//! and default options for embedding and prompt models. The module is conditionally compiled based
//! on the "openai" feature flag.

use async_openai::error::OpenAIError;
use async_openai::types::chat::CreateChatCompletionRequestArgs;
use async_openai::types::embeddings::CreateEmbeddingRequestArgs;
use derive_builder::Builder;
use serde::Serialize;
use std::pin::Pin;
use std::sync::Arc;
use swiftide_core::chat_completion::Usage;
use swiftide_core::chat_completion::errors::LanguageModelError;

mod chat_completion;
mod embed;
mod provider_error;
mod responses_api;
mod simple_prompt;
mod structured_prompt;
mod tool_schema;

// expose type aliases to simplify downstream use of the open ai builder invocations
pub use async_openai::config::AzureConfig;
pub use async_openai::config::OpenAIConfig;
pub use async_openai::types::responses::ReasoningEffort;

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
///     .api_key("my-api-key")
///     .build().unwrap();
///
/// // Point it at an OpenAI-compatible endpoint, such as a local gateway, vLLM or LiteLLM.
/// let openai = OpenAI::builder()
///     .api_base("http://127.0.0.1:8080/v1")
///     .api_key("my-api-key")
///     .default_prompt_model("qwen3-coder")
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
        field(ty = "ClientSource<C>", build = "self.client.build()"),
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

    /// Opt-in flag to use `OpenAI`'s Responses API instead of the legacy Chat Completions API.
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

/// Where the builder gets its `async_openai` client from.
///
/// Keeps an explicitly provided client apart from one assembled with the builder's connection
/// setters, so neither silently overwrites the other: whichever is set last wins.
#[derive(Default)]
enum ClientSource<C: async_openai::config::Config> {
    /// `async_openai`'s default client. For [`OpenAIConfig`] that reads `OPENAI_API_KEY` and
    /// `OPENAI_BASE_URL` from the environment.
    #[default]
    Default,
    /// Built by the builder from the connection setters (`api_base`, `api_key`).
    Configured(Arc<async_openai::Client<C>>),
    /// Supplied as a whole with `client(..)`.
    Explicit(Arc<async_openai::Client<C>>),
}

impl<C: async_openai::config::Config + Default> ClientSource<C> {
    fn build(&self) -> Arc<async_openai::Client<C>> {
        match self {
            Self::Default => Arc::new(async_openai::Client::<C>::default()),
            Self::Configured(client) | Self::Explicit(client) => Arc::clone(client),
        }
    }
}

impl<C: async_openai::config::Config> Clone for ClientSource<C> {
    fn clone(&self) -> Self {
        match self {
            Self::Default => Self::Default,
            Self::Configured(client) => Self::Configured(Arc::clone(client)),
            Self::Explicit(client) => Self::Explicit(Arc::clone(client)),
        }
    }
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

    /// Request reasoning summaries and encrypted content from the Responses API.
    ///
    /// This is enabled by default, but only takes effect when `reasoning_effort` is set.
    /// Disable it with `reasoning_features(false)` to omit those requests. Existing reasoning
    /// history is still replayed, independently of this setting and `reasoning_effort`.
    ///
    /// Note: reasoning summaries/encrypted content require an `OpenAI` organization that is
    /// verified for reasoning access; unverified orgs may receive no summaries.
    #[builder(default, setter(into))]
    pub reasoning_features: Option<bool>,

    /// Replay plaintext reasoning, summaries, and status in Responses API conversation history.
    ///
    /// Enable only for compatible providers that accept plaintext reasoning input. Defaults to
    /// false for `OpenAI` and true when using `OpenRouter::builder()`. Encrypted reasoning is
    /// replayed independently of this option and the current turn's reasoning effort.
    /// This option has no effect on Chat Completions.
    #[builder(default)]
    pub replay_plaintext_reasoning: Option<bool>,

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

    /// Additional top-level JSON fields for OpenAI-compatible providers.
    #[builder(default, setter(custom))]
    extra_body: serde_json::Map<String, serde_json::Value>,

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
        if let Some(reasoning_features) = other.reasoning_features {
            self.reasoning_features = Some(reasoning_features);
        }
        if let Some(replay_plaintext_reasoning) = other.replay_plaintext_reasoning {
            self.replay_plaintext_reasoning = Some(replay_plaintext_reasoning);
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
        self.extra_body.extend(other.extra_body.clone());
        if let Some(dimensions) = other.dimensions {
            self.dimensions = Some(dimensions);
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
            reasoning_features: value.reasoning_features.flatten(),
            replay_plaintext_reasoning: value.replay_plaintext_reasoning.flatten(),
            presence_penalty: value.presence_penalty.flatten(),
            seed: value.seed.flatten(),
            metadata: value.metadata.flatten(),
            user: value.user.flatten(),
            extra_body: value.extra_body.unwrap_or_default(),
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
            reasoning_features: value.reasoning_features.flatten(),
            replay_plaintext_reasoning: value.replay_plaintext_reasoning.flatten(),
            presence_penalty: value.presence_penalty.flatten(),
            seed: value.seed.flatten(),
            metadata: value.metadata.flatten(),
            user: value.user.flatten(),
            extra_body: value.extra_body.unwrap_or_default(),
            dimensions: value.dimensions.flatten(),
        }
    }
}

impl OptionsBuilder {
    /// Adds additional top-level JSON fields for OpenAI-compatible providers.
    pub fn extra_body(
        &mut self,
        extra_body: serde_json::Map<String, serde_json::Value>,
    ) -> &mut Self {
        self.extra_body = Some(extra_body);
        self
    }
}

pub(crate) fn request_body_with_extra_body<T: Serialize>(
    request: &T,
    extra_body: &serde_json::Map<String, serde_json::Value>,
) -> Result<serde_json::Value, LanguageModelError> {
    let mut body = serde_json::to_value(request).map_err(LanguageModelError::permanent)?;

    if extra_body.is_empty() {
        return Ok(body);
    }

    let object = body.as_object_mut().ok_or_else(|| {
        LanguageModelError::permanent("OpenAI request must serialize to a JSON object")
    })?;

    object.extend(extra_body.clone());

    Ok(body)
}

impl OpenAI {
    /// Creates a new `OpenAIBuilder` for constructing `OpenAI` instances.
    pub fn builder() -> OpenAIBuilder {
        let mut builder = OpenAIBuilder::default();
        builder.default_options(Options {
            reasoning_features: Some(true),
            ..Default::default()
        });
        builder
    }
}

impl OpenAIBuilder {
    /// Sends requests to `api_base` instead of `https://api.openai.com/v1`.
    ///
    /// Use this to point `OpenAI` at any OpenAI-compatible endpoint, such as a local gateway,
    /// vLLM or `LiteLLM`. The url includes the version prefix, e.g. `http://127.0.0.1:8080/v1`.
    ///
    /// `api_base` and [`api_key`](Self::api_key) configure the client the builder creates for
    /// you; anything not set falls back to `async_openai`'s defaults (`OPENAI_BASE_URL`,
    /// `OPENAI_API_KEY`). A client passed to [`client`](GenericOpenAIBuilder::client) replaces
    /// that configuration entirely. Whichever is called last wins: calling `api_base` after
    /// `client` discards the explicit client and starts from the defaults again.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use swiftide_integrations::openai::OpenAI;
    /// let openai = OpenAI::builder()
    ///     .api_base("http://127.0.0.1:8080/v1")
    ///     .api_key("my-api-key")
    ///     .default_prompt_model("qwen3-coder")
    ///     .build()
    ///     .unwrap();
    /// ```
    pub fn api_base(&mut self, api_base: impl Into<String>) -> &mut Self {
        self.configure_client(|config| config.with_api_base(api_base))
    }

    /// Authenticates requests with `api_key` instead of `OPENAI_API_KEY`.
    ///
    /// See [`api_base`](Self::api_base) for how this combines with
    /// [`client`](GenericOpenAIBuilder::client).
    pub fn api_key(&mut self, api_key: impl Into<String>) -> &mut Self {
        self.configure_client(|config| config.with_api_key(api_key))
    }

    /// Applies `update` to the config of the client the builder creates, starting from the
    /// defaults unless an earlier connection setter already configured it.
    fn configure_client(&mut self, update: impl FnOnce(OpenAIConfig) -> OpenAIConfig) -> &mut Self {
        let config = match &self.client {
            ClientSource::Configured(client) => client.config().clone(),
            ClientSource::Default | ClientSource::Explicit(_) => OpenAIConfig::default(),
        };
        self.client =
            ClientSource::Configured(Arc::new(async_openai::Client::with_config(update(config))));
        self
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
    /// Use this when you need full control over the client, such as a custom http client,
    /// backoff or config type. To only change the endpoint or key of `OpenAI`, prefer
    /// [`OpenAIBuilder::api_base`] and [`OpenAIBuilder::api_key`].
    ///
    /// The client replaces anything configured with `api_base` or `api_key` before it. Whichever
    /// is called last wins.
    ///
    /// # Parameters
    /// - `client`: The `OpenAI` client to set.
    ///
    /// # Returns
    /// A mutable reference to the `OpenAIBuilder`.
    pub fn client(&mut self, client: async_openai::Client<C>) -> &mut Self {
        self.client = ClientSource::Explicit(Arc::new(client));
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
    provider_error::openai_error_to_language_model_error(e)
}

#[cfg(test)]
mod test {
    use super::*;
    use async_openai::error::{ApiError, ApiErrorResponse, OpenAIError, StreamError};
    use eventsource_stream::Event;
    use reqwest::StatusCode;

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

    fn api_base_of(openai: &OpenAI) -> &str {
        use async_openai::config::Config as _;
        openai.client().config().api_base()
    }

    fn authorization_of(openai: &OpenAI) -> Option<String> {
        use async_openai::config::Config as _;
        openai
            .client()
            .config()
            .headers()
            .get(reqwest::header::AUTHORIZATION)
            .map(|value| value.to_str().unwrap().to_string())
    }

    #[test_log::test(tokio::test)]
    async fn test_api_base_and_api_key_send_requests_to_compatible_endpoint() {
        use swiftide_core::SimplePrompt as _;
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("authorization", "Bearer test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "chatcmpl-1",
                "object": "chat.completion",
                "created": 0,
                "model": "local-model",
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "Hello from the gateway"},
                    "finish_reason": "stop"
                }]
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let openai = OpenAI::builder()
            .api_base(mock_server.uri())
            .api_key("test")
            .default_prompt_model("local-model")
            .build()
            .unwrap();

        let answer = openai.prompt("Hi".into()).await.unwrap();

        assert_eq!(answer, "Hello from the gateway");
    }

    #[test]
    fn test_api_base_and_api_key_combine_in_any_order() {
        let openai = OpenAI::builder()
            .api_key("test")
            .api_base("http://gateway.local/v1")
            .build()
            .unwrap();

        assert_eq!(api_base_of(&openai), "http://gateway.local/v1");
        assert_eq!(authorization_of(&openai).as_deref(), Some("Bearer test"));
    }

    #[test]
    fn test_explicit_client_replaces_earlier_api_base() {
        let explicit = async_openai::Client::with_config(
            OpenAIConfig::new().with_api_base("http://explicit.local/v1"),
        );

        let openai = OpenAI::builder()
            .api_base("http://gateway.local/v1")
            .client(explicit)
            .build()
            .unwrap();

        assert_eq!(api_base_of(&openai), "http://explicit.local/v1");
    }

    #[test]
    fn test_api_base_replaces_earlier_explicit_client() {
        let explicit = async_openai::Client::with_config(
            OpenAIConfig::new()
                .with_api_base("http://explicit.local/v1")
                .with_api_key("explicit-key"),
        );

        let openai = OpenAI::builder()
            .client(explicit)
            .api_base("http://gateway.local/v1")
            .build()
            .unwrap();

        assert_eq!(api_base_of(&openai), "http://gateway.local/v1");
        assert_ne!(
            authorization_of(&openai).as_deref(),
            Some("Bearer explicit-key")
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

        let openai_error = OpenAIError::ApiError(ApiErrorResponse {
            status_code: StatusCode::BAD_REQUEST,
            api_error,
        });
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

        let openai_error = OpenAIError::ApiError(ApiErrorResponse {
            status_code: StatusCode::UNAUTHORIZED,
            api_error,
        });
        let result = openai_error_to_language_model_error(openai_error);

        // Verify it's categorized as PermanentError
        match result {
            LanguageModelError::PermanentError(_) => {} // Expected
            _ => panic!("Expected PermanentError, got {result:?}"),
        }
    }

    #[test]
    fn test_rate_limit_api_error_is_transient() {
        let api_error = ApiError {
            message: "Rate limit exceeded".to_string(),
            r#type: Some("rate_limit_error".to_string()),
            param: None,
            code: None,
        };

        let openai_error = OpenAIError::ApiError(ApiErrorResponse {
            status_code: StatusCode::TOO_MANY_REQUESTS,
            api_error,
        });
        let result = openai_error_to_language_model_error(openai_error);

        assert!(matches!(result, LanguageModelError::TransientError(_)));
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
        let openai_error =
            OpenAIError::StreamError(Box::new(StreamError::UnknownEvent(Event::default())));
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

    #[test]
    fn test_options_merge_overrides_set_fields() {
        let mut base = Options::builder()
            .prompt_model("a")
            .temperature(0.1)
            .build()
            .unwrap();

        let overlay = Options::builder()
            .prompt_model("b")
            .presence_penalty(0.2)
            .build()
            .unwrap();

        base.merge(&overlay);

        assert_eq!(base.prompt_model.as_deref(), Some("b"));
        assert_eq!(base.temperature, Some(0.1));
        assert_eq!(base.presence_penalty, Some(0.2));
    }

    #[test]
    #[allow(deprecated)]
    fn test_chat_completion_request_defaults_sends_reasoning_effort() {
        let openai: OpenAI = OpenAI::builder()
            .default_options(
                Options::builder()
                    .parallel_tool_calls(true)
                    .max_completion_tokens(42)
                    .temperature(0.3)
                    .reasoning_effort(ReasoningEffort::Low)
                    .seed(7)
                    .presence_penalty(1.1)
                    .metadata(serde_json::json!({"tag": "demo"}))
                    .user("user-1"),
            )
            .build()
            .unwrap();

        let built = openai
            .chat_completion_request_defaults()
            .messages(Vec::new())
            .model("gpt-4o")
            .build()
            .unwrap();

        assert_eq!(built.parallel_tool_calls, Some(true));
        assert_eq!(built.max_completion_tokens, Some(42));
        assert_eq!(built.temperature, Some(0.3));
        assert_eq!(built.reasoning_effort, Some(ReasoningEffort::Low));
        assert_eq!(built.seed, Some(7));
        assert_eq!(built.presence_penalty, Some(1.1));
        assert_eq!(
            built.metadata,
            Some(async_openai::types::Metadata::from(
                serde_json::json!({"tag": "demo"})
            ))
        );
        assert_eq!(built.user, Some("user-1".to_string()));
    }

    #[test]
    #[allow(deprecated)]
    fn test_embed_request_defaults_sets_user_and_dimensions() {
        let openai: OpenAI = OpenAI::builder()
            .default_options(Options::builder().user("end-user").dimensions(128))
            .build()
            .unwrap();

        let built = openai
            .embed_request_defaults()
            .model("text-embedding-3-small")
            .input("hello")
            .build()
            .unwrap();
        assert_eq!(built.user, Some("end-user".to_string()));
        assert_eq!(built.dimensions, Some(128));
    }
}
