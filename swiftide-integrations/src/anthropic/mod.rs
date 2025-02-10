use std::sync::Arc;

use derive_builder::Builder;

pub mod chat_completion;
pub mod simple_prompt;

#[derive(Debug, Builder, Clone)]
pub struct Anthropic {
    #[builder(
        default = Arc::new(async_anthropic::Client::default()),
        setter(custom)
    )]
    client: Arc<async_anthropic::Client>,

    #[builder(default)]
    default_options: Options,
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
