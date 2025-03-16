//! This module provides integration with `OpenRouter`'s API, enabling the use of language models
//! and embeddings within the Swiftide project. It includes the `OpenRouter` struct for managing API
//! clients and default options for embedding and prompt models. The module is conditionally
//! compiled based on the "openrouter" feature flag.

use config::OpenRouterConfig;
use derive_builder::Builder;
use std::sync::Arc;

pub mod chat_completion;
pub mod config;
pub mod simple_prompt;

/// The `OpenRouter` struct encapsulates an `OpenRouter` client and default options for embedding
/// and prompt models. It uses the `Builder` pattern for flexible and customizable instantiation.
///
/// By default it will look for a `OPENROUTER_API_KEY` environment variable. Note that either a
/// prompt model or embedding model always need to be set, either with
/// [`OpenRouter::with_default_prompt_model`] or [`OpenRouter::with_default_embed_model`] or via the
/// builder. You can find available models in the `OpenRouter` documentation.
///
/// Under the hood it uses [`async_openai`], with the `OpenRouter` openai compatible api. This means
/// some features might not work as expected. See the `OpenRouter` documentation for details.
#[derive(Debug, Builder, Clone)]
#[builder(setter(into, strip_option))]
pub struct OpenRouter {
    /// The `OpenRouter` client, wrapped in an `Arc` for thread-safe reference counting.
    #[builder(default = "default_client()", setter(custom))]
    client: Arc<async_openai::Client<OpenRouterConfig>>,
    /// Default options for the embedding and prompt models.
    #[builder(default)]
    default_options: Options,
}

impl Default for OpenRouter {
    fn default() -> Self {
        Self {
            client: default_client(),
            default_options: Options::default(),
        }
    }
}

/// The `Options` struct holds configuration options for the `OpenRouter` client.
/// It includes optional fields for specifying the embedding and prompt models.
#[derive(Debug, Default, Clone, Builder)]
#[builder(setter(into, strip_option))]
pub struct Options {
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

impl OpenRouter {
    /// Creates a new `OpenRouterBuilder` for constructing `OpenRouter` instances.
    pub fn builder() -> OpenRouterBuilder {
        OpenRouterBuilder::default()
    }

    /// Sets a default prompt model to use when prompting
    pub fn with_default_prompt_model(&mut self, model: impl Into<String>) -> &mut Self {
        self.default_options = Options {
            prompt_model: Some(model.into()),
        };
        self
    }
}

impl OpenRouterBuilder {
    /// Sets the `OpenRouter` client for the `OpenRouter` instance.
    ///
    /// # Parameters
    /// - `client`: The `OpenRouter` client to set.
    ///
    /// # Returns
    /// A mutable reference to the `OpenRouterBuilder`.
    pub fn client(&mut self, client: async_openai::Client<OpenRouterConfig>) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }

    /// Sets the default prompt model for the `OpenRouter` instance.
    ///
    /// # Parameters
    /// - `model`: The prompt model to set.
    ///
    /// # Returns
    /// A mutable reference to the `OpenRouterBuilder`.
    pub fn default_prompt_model(&mut self, model: impl Into<String>) -> &mut Self {
        if let Some(options) = self.default_options.as_mut() {
            options.prompt_model = Some(model.into());
        } else {
            self.default_options = Some(Options {
                prompt_model: Some(model.into()),
            });
        }
        self
    }
}

fn default_client() -> Arc<async_openai::Client<OpenRouterConfig>> {
    Arc::new(async_openai::Client::with_config(
        OpenRouterConfig::default(),
    ))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_default_prompt_model() {
        let openai = OpenRouter::builder()
            .default_prompt_model("llama3.1")
            .build()
            .unwrap();
        assert_eq!(
            openai.default_options.prompt_model,
            Some("llama3.1".to_string())
        );
    }

    #[test]
    fn test_default_models() {
        let openrouter = OpenRouter::builder()
            .default_prompt_model("llama3.1")
            .build()
            .unwrap();
        assert_eq!(
            openrouter.default_options.prompt_model,
            Some("llama3.1".to_string())
        );
    }

    #[test]
    fn test_building_via_default_prompt_model() {
        let mut client = OpenRouter::default();

        assert!(client.default_options.prompt_model.is_none());

        client.with_default_prompt_model("llama3.1");
        assert_eq!(
            client.default_options.prompt_model,
            Some("llama3.1".to_string())
        );
    }
}
