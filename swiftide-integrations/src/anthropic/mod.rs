use std::{pin::Pin, sync::Arc};

use derive_builder::Builder;
use swiftide_core::chat_completion::Usage;

pub mod chat_completion;
pub mod simple_prompt;
mod tool_schema;

#[derive(Builder, Clone)]
pub struct Anthropic {
    #[builder(
        default = Arc::new(default_client()),
        setter(custom)
    )]
    client: Arc<async_anthropic::Client>,

    #[builder(default)]
    default_options: Options,

    #[cfg(feature = "metrics")]
    #[builder(default)]
    /// Optional metadata to attach to metrics emitted by this client.
    metric_metadata: Option<std::collections::HashMap<String, String>>,

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

impl std::fmt::Debug for Anthropic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Anthropic")
            .field("client", &self.client)
            .field("default_options", &self.default_options)
            .finish()
    }
}

#[derive(Debug, Clone, Builder)]
#[builder(setter(into, strip_option))]
pub struct Options {
    #[builder(default)]
    pub prompt_model: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            prompt_model: "claude-3-5-sonnet-20241022".to_string(),
        }
    }
}

/// The default `async_anthropic` client, using rustls when the `rustls` feature is enabled.
///
/// `async_anthropic::Client::default()` uses `reqwest::Client::new()`, which picks native TLS when
/// another crate enables it on reqwest 0.12 (for example `fastembed`). Selecting rustls explicitly
/// keeps the TLS policy the same for every integration.
#[cfg(feature = "rustls")]
fn default_client() -> async_anthropic::Client {
    // Mirrors `async_anthropic::Client::default()` (async-anthropic 0.6), except for the HTTP
    // client.
    let backoff = backoff::ExponentialBackoffBuilder::default()
        .with_initial_interval(std::time::Duration::from_secs(15))
        .with_multiplier(2.0)
        .with_randomization_factor(0.05)
        .with_max_elapsed_time(Some(std::time::Duration::from_mins(2)))
        .build();
    let http_client = reqwest_0_12::Client::builder()
        .use_rustls_tls()
        .build()
        .expect("failed to build the Anthropic HTTP client");

    async_anthropic::Client::builder()
        .http_client(http_client)
        .base_url("https://api.anthropic.com")
        .version("2023-06-01")
        .backoff(backoff)
        .build()
        .expect("every Anthropic client field has a value")
}

#[cfg(not(feature = "rustls"))]
fn default_client() -> async_anthropic::Client {
    async_anthropic::Client::default()
}

impl Anthropic {
    pub fn builder() -> AnthropicBuilder {
        AnthropicBuilder::default()
    }
}

impl AnthropicBuilder {
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

    /// Sets the client for the `Anthropic` instance.
    ///
    /// See the `async_anthropic::Client` documentation for more information.
    ///
    /// # Parameters
    /// - `client`: The `Anthropic` client to set.
    ///
    /// # Returns
    /// A mutable reference to the `AnthropicBuilder`.
    pub fn client(&mut self, client: async_anthropic::Client) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }

    /// Sets the default prompt model for the `Anthropic` instance.
    ///
    /// # Parameters
    /// - `model`: The prompt model to set.
    ///
    /// # Returns
    /// A mutable reference to the `AnthropicBuilder`.
    pub fn default_prompt_model(&mut self, model: impl Into<String>) -> &mut Self {
        if let Some(options) = self.default_options.as_mut() {
            options.prompt_model = model.into();
        } else {
            self.default_options = Some(Options {
                prompt_model: model.into(),
            });
        }
        self
    }
}
