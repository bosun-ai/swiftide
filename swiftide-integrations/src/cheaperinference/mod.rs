//! This module provides integration with `Cheaper Inference`'s API, enabling the use of language
//! models within the Swiftide project. It includes the `CheaperInference` struct for managing API
//! clients and default options for prompt models. The module is conditionally compiled based on the
//! "cheaperinference" feature flag.

use crate::openai;

use self::config::CheaperInferenceConfig;

mod config;

/// The `CheaperInference` struct encapsulates a `Cheaper Inference` client that implements
/// [`swiftide_core::SimplePrompt`]
///
/// There is also a builder available.
///
/// By default it will look for a `CHEAPER_INFERENCE_API_KEY` environment variable. Note that a model
/// always needs to be set, either with [`CheaperInference::with_default_prompt_model`] or via the
/// builder. You can find available models in the Cheaper Inference documentation.
///
/// Under the hood it uses [`async_openai`], with the Cheaper Inference openai compatible api. This
/// means some features might not work as expected. See the Cheaper Inference documentation for
/// details.
pub type CheaperInference = openai::GenericOpenAI<CheaperInferenceConfig>;
pub type CheaperInferenceBuilder = openai::GenericOpenAIBuilder<CheaperInferenceConfig>;
pub type CheaperInferenceBuilderError = openai::GenericOpenAIBuilderError;
pub use openai::{Options, OptionsBuilder, OptionsBuilderError};

impl CheaperInference {
    pub fn builder() -> CheaperInferenceBuilder {
        CheaperInferenceBuilder::default()
    }
}

impl Default for CheaperInference {
    fn default() -> Self {
        Self::builder().build().unwrap()
    }
}
