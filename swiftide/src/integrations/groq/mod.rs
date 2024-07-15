//! This module provides integration with `Groq`'s API, enabling the use of language models within the Swiftide project.
//! It includes the `Groq` struct for managing API clients and default options for prompt models.
//! The module is conditionally compiled based on the "groq" feature flag.

use derive_builder::Builder;
use std::sync::Arc;

use self::config::GroqConfig;

mod config;
mod simple_prompt;

/// The `Groq` struct encapsulates an `Groq` client and default options for  prompt models.
/// It uses the `Builder` pattern for flexible and customizable instantiation.
#[derive(Debug, Builder, Clone)]
#[builder(setter(into, strip_option))]
pub struct Groq {
    /// The `Groq` client, wrapped in an `Arc` for thread-safe reference counting.
    /// Defaults to a new instance of `async_openai::Client`.
    #[builder(default = "self.default_client()", setter(custom))]
    client: Arc<async_openai::Client<GroqConfig>>,
    /// Default options for prompt models.
    #[builder(default)]
    default_options: Options,
}

/// The `Options` struct holds configuration options for the `Groq` client.
/// It includes optional fields for specifying the prompt models.
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

impl Groq {
    /// Creates a new `GroqBuilder` for constructing `Groq` instances.
    pub fn builder() -> GroqBuilder {
        GroqBuilder::default()
    }
}

impl GroqBuilder {
    #[allow(clippy::unused_self)]
    fn default_client(&self) -> Arc<async_openai::Client<GroqConfig>> {
        async_openai::Client::with_config(GroqConfig::default()).into()
    }

    /// Sets the `Groq` client for the `Groq` instance.
    ///
    /// # Parameters
    /// - `client`: The `Groq` client to set.
    ///
    /// # Returns
    /// A mutable reference to the `GroqBuilder`.
    pub fn client(&mut self, client: async_openai::Client<GroqConfig>) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }

    /// Sets the default prompt model for the `Groq` instance.
    ///
    /// # Parameters
    /// - `model`: The prompt model to set.
    ///
    /// # Returns
    /// A mutable reference to the `GroqBuilder`.
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

#[cfg(test)]
mod test {
    use super::*;

    /// test default model
    #[test]
    fn test_default_prompt_model() {
        let openai = Groq::builder()
            .default_prompt_model("llama3-8b-8192")
            .build()
            .unwrap();
        assert_eq!(
            openai.default_options.prompt_model,
            Some("llama3-8b-8192".to_string())
        );

        let openai = Groq::builder()
            .default_prompt_model("llama3-8b-8192")
            .build()
            .unwrap();
        assert_eq!(
            openai.default_options.prompt_model,
            Some("llama3-8b-8192".to_string())
        );
    }
}
