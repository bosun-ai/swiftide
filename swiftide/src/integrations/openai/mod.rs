//! This module provides integration with OpenAI's API, enabling the use of language models and embeddings within the Swiftide project.
//! It includes the `OpenAI` struct for managing API clients and default options for embedding and prompt models.
//! The module is conditionally compiled based on the "openai" feature flag.

use derive_builder::Builder;
use std::sync::Arc;

mod embed;
mod simple_prompt;

/// The `OpenAI` struct encapsulates an OpenAI client and default options for embedding and prompt models.
/// It uses the `Builder` pattern for flexible and customizable instantiation.
#[derive(Debug, Builder, Clone)]
#[builder(setter(into, strip_option))]
pub struct OpenAI {
    /// The OpenAI client, wrapped in an `Arc` for thread-safe reference counting.
    /// Defaults to a new instance of `async_openai::Client`.
    #[builder(default = "Arc::new(async_openai::Client::new())", setter(custom))]
    client: Arc<async_openai::Client<async_openai::config::OpenAIConfig>>,
    /// Default options for embedding and prompt models.
    #[builder(default)]
    default_options: Options,
}

/// The `Options` struct holds configuration options for the `OpenAI` client.
/// It includes optional fields for specifying the embedding and prompt models.
#[derive(Debug, Default, Clone, Builder)]
#[builder(setter(into, strip_option))]
pub struct Options {
    /// The default embedding model to use, if specified.
    #[builder(default)]
    pub embed_model: Option<String>,
    /// The default prompt model to use, if specified.
    #[builder(default)]
    pub prompt_model: Option<String>,
}

impl Options {
    /// Creates a new `OptionsBuilder` for constructing `Options` instances.
    pub fn builder() -> OptionsBuilder {
        OptionsBuilder::default()
    }
}

impl OpenAI {
    /// Creates a new `OpenAIBuilder` for constructing `OpenAI` instances.
    pub fn builder() -> OpenAIBuilder {
        OpenAIBuilder::default()
    }
}

impl OpenAIBuilder {
    /// Sets the OpenAI client for the `OpenAI` instance.
    ///
    /// # Parameters
    /// - `client`: The OpenAI client to set.
    ///
    /// # Returns
    /// A mutable reference to the `OpenAIBuilder`.
    pub fn client(
        &mut self,
        client: async_openai::Client<async_openai::config::OpenAIConfig>,
    ) -> &mut Self {
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
}
