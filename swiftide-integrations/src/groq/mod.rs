//! This module provides integration with `Groq`'s API, enabling the use of language models within
//! the Swiftide project. It includes the `Groq` struct for managing API clients and default options
//! for prompt models. The module is conditionally compiled based on the "groq" feature flag.

use crate::openai;

use self::config::GroqConfig;

mod config;

/// The `Groq` struct encapsulates a `Groq` client that implements [`swiftide_core::SimplePrompt`]
///
/// There is also a builder available.
///
/// By default it will look for a `GROQ_API_KEY` environment variable. Note that a model
/// always needs to be set, either with [`Groq::with_default_prompt_model`] or via the builder.
/// You can find available models in the Groq documentation.
///
/// Under the hood it uses [`async_openai`], with the Groq openai mapping. This means
/// some features might not work as expected. See the Groq documentation for details.
pub type Groq = openai::GenericOpenAI<GroqConfig>;
pub type GroqBuilder = openai::GenericOpenAIBuilder<GroqConfig>;
pub type GroqBuilderError = openai::GenericOpenAIBuilderError;
pub use openai::{Options, OptionsBuilder, OptionsBuilderError};

impl Groq {
    pub fn builder() -> GroqBuilder {
        GroqBuilder::default()
    }
}

impl Default for Groq {
    fn default() -> Self {
        Self::builder().build().unwrap()
    }
}
