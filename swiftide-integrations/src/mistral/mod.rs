//! This module provides integration with `Mistral AI`'s API, enabling the use of language models
//! within the Swiftide project. It includes the `Mistral` struct for managing API clients and
//! default options for prompt models. The module is conditionally compiled based on the "mistral"
//! feature flag.

use crate::openai;

use self::config::MistralConfig;

mod config;

/// The `Mistral` struct encapsulates a Mistral AI client that implements
/// [`swiftide_core::SimplePrompt`].
///
/// There is also a builder available.
///
/// By default it will look for a `MISTRAL_API_KEY` environment variable. Note that a model always
/// needs to be set, either with [`Mistral::with_default_prompt_model`] or via the builder. You can
/// find available models in the Mistral AI documentation.
///
/// Under the hood it uses [`async_openai`] against Mistral AI's OpenAI-compatible chat completion
/// endpoint. This means some features might not work as expected. See the Mistral AI documentation
/// for details.
pub type Mistral = openai::GenericOpenAI<MistralConfig>;
pub type MistralBuilder = openai::GenericOpenAIBuilder<MistralConfig>;
pub type MistralBuilderError = openai::GenericOpenAIBuilderError;
pub use openai::{Options, OptionsBuilder, OptionsBuilderError};

impl Mistral {
    pub fn builder() -> MistralBuilder {
        MistralBuilder::default()
    }
}

impl Default for Mistral {
    fn default() -> Self {
        Self::builder().build().unwrap()
    }
}
