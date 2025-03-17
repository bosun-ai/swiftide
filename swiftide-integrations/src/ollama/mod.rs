//! This module provides integration with `Ollama`'s API, enabling the use of language models and
//! embeddings within the Swiftide project. It includes the `Ollama` struct for managing API clients
//! and default options for embedding and prompt models. The module is conditionally compiled based
//! on the "ollama" feature flag.

use config::OllamaConfig;
use derive_builder::Builder;
use std::sync::Arc;

pub mod chat_completion;
pub mod config;
pub mod embed;
pub mod simple_prompt;

/// The `Ollama` struct encapsulates an `Ollama` client and default options for embedding and prompt
/// models. It uses the `Builder` pattern for flexible and customizable instantiation.
///
/// By default it will look for a `OLLAMA_API_KEY` environment variable. Note that either a prompt
/// model or embedding model always need to be set, either with
/// [`Ollama::with_default_prompt_model`] or [`Ollama::with_default_embed_model`] or via the
/// builder. You can find available models in the Ollama documentation.
///
/// Under the hood it uses [`async_openai`], with the Ollama openai mapping. This means
/// some features might not work as expected. See the Ollama documentation for details.
#[derive(Debug, Builder, Clone)]
#[builder(setter(into, strip_option))]
pub struct Ollama {
    /// The `Ollama` client, wrapped in an `Arc` for thread-safe reference counting.
    #[builder(default = "default_client()", setter(custom))]
    client: Arc<async_openai::Client<OllamaConfig>>,
    /// Default options for the embedding and prompt models.
    #[builder(default)]
    default_options: Options,
}

impl Default for Ollama {
    fn default() -> Self {
        Self {
            client: default_client(),
            default_options: Options::default(),
        }
    }
}

/// The `Options` struct holds configuration options for the `Ollama` client.
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

impl Ollama {
    /// Creates a new `OllamaBuilder` for constructing `Ollama` instances.
    pub fn builder() -> OllamaBuilder {
        OllamaBuilder::default()
    }

    /// Sets a default prompt model to use when prompting
    pub fn with_default_prompt_model(&mut self, model: impl Into<String>) -> &mut Self {
        self.default_options = Options {
            prompt_model: Some(model.into()),
            embed_model: self.default_options.embed_model.clone(),
        };
        self
    }

    /// Sets a default embedding model to use when embedding
    pub fn with_default_embed_model(&mut self, model: impl Into<String>) -> &mut Self {
        self.default_options = Options {
            prompt_model: self.default_options.prompt_model.clone(),
            embed_model: Some(model.into()),
        };
        self
    }
}

impl OllamaBuilder {
    /// Sets the `Ollama` client for the `Ollama` instance.
    ///
    /// # Parameters
    /// - `client`: The `Ollama` client to set.
    ///
    /// # Returns
    /// A mutable reference to the `OllamaBuilder`.
    pub fn client(&mut self, client: async_openai::Client<OllamaConfig>) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }

    /// Sets the default embedding model for the `Ollama` instance.
    ///
    /// # Parameters
    /// - `model`: The embedding model to set.
    ///
    /// # Returns
    /// A mutable reference to the `OllamaBuilder`.
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

    /// Sets the default prompt model for the `Ollama` instance.
    ///
    /// # Parameters
    /// - `model`: The prompt model to set.
    ///
    /// # Returns
    /// A mutable reference to the `OllamaBuilder`.
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

fn default_client() -> Arc<async_openai::Client<OllamaConfig>> {
    Arc::new(async_openai::Client::with_config(OllamaConfig::default()))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_default_prompt_model() {
        let openai = Ollama::builder()
            .default_prompt_model("llama3.1")
            .build()
            .unwrap();
        assert_eq!(
            openai.default_options.prompt_model,
            Some("llama3.1".to_string())
        );
    }

    #[test]
    fn test_default_embed_model() {
        let ollama = Ollama::builder()
            .default_embed_model("mxbai-embed-large")
            .build()
            .unwrap();
        assert_eq!(
            ollama.default_options.embed_model,
            Some("mxbai-embed-large".to_string())
        );
    }

    #[test]
    fn test_default_models() {
        let ollama = Ollama::builder()
            .default_embed_model("mxbai-embed-large")
            .default_prompt_model("llama3.1")
            .build()
            .unwrap();
        assert_eq!(
            ollama.default_options.embed_model,
            Some("mxbai-embed-large".to_string())
        );
        assert_eq!(
            ollama.default_options.prompt_model,
            Some("llama3.1".to_string())
        );
    }

    #[test]
    fn test_building_via_default_prompt_model() {
        let mut client = Ollama::default();

        assert!(client.default_options.prompt_model.is_none());

        client.with_default_prompt_model("llama3.1");
        assert_eq!(
            client.default_options.prompt_model,
            Some("llama3.1".to_string())
        );
    }

    #[test]
    fn test_building_via_default_embed_model() {
        let mut client = Ollama::default();

        assert!(client.default_options.embed_model.is_none());

        client.with_default_embed_model("mxbai-embed-large");
        assert_eq!(
            client.default_options.embed_model,
            Some("mxbai-embed-large".to_string())
        );
    }

    #[test]
    fn test_building_via_default_models() {
        let mut client = Ollama::default();

        assert!(client.default_options.embed_model.is_none());

        client.with_default_prompt_model("llama3.1");
        client.with_default_embed_model("mxbai-embed-large");
        assert_eq!(
            client.default_options.prompt_model,
            Some("llama3.1".to_string())
        );
        assert_eq!(
            client.default_options.embed_model,
            Some("mxbai-embed-large".to_string())
        );
    }
}
