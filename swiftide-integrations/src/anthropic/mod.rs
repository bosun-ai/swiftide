use std::{pin::Pin, sync::Arc};

use derive_builder::Builder;
use swiftide_core::chat_completion::Usage;

pub mod chat_completion;
pub mod simple_prompt;

#[derive(Builder, Clone)]
pub struct Anthropic {
    #[builder(
        default = Arc::new(async_anthropic::Client::default()),
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
                    Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + Sync + 'a>,
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
            ) -> Pin<
                Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + Sync + 'a>,
            > + Send
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
