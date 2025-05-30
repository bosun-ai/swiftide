//! This module provides integration with `Gemini`'s API, enabling the use of language models within
//! the Swiftide project. It includes the `Gemini` struct for managing API clients and default
//! options for prompt models. The module is conditionally compiled based on the "groq" feature
//! flag.

use crate::openai;

use self::config::GeminiConfig;

mod config;

/// The `Gemini` struct encapsulates a `Gemini` client that implements
/// [`swiftide_core::SimplePrompt`]
///
/// There is also a builder available.
///
/// By default it will look for a `GEMINI_API_KEY` environment variable. Note that a model
/// always needs to be set, either with [`Gemini::with_default_prompt_model`] or via the builder.
/// You can find available models in the Gemini documentation.
///
/// Under the hood it uses [`async_openai`], with the Gemini openai mapping. This means
/// some features might not work as expected. See the Gemini documentation for details.
pub type Gemini = openai::GenericOpenAI<GeminiConfig>;
pub type GeminiBuilder = openai::GenericOpenAIBuilder<GeminiConfig>;
pub type GeminiBuilderError = openai::GenericOpenAIBuilderError;
pub use openai::{Options, OptionsBuilder, OptionsBuilderError};

impl Gemini {
    pub fn builder() -> GeminiBuilder {
        GeminiBuilder::default()
    }
}

impl Default for Gemini {
    fn default() -> Self {
        Self::builder().build().unwrap()
    }
}
