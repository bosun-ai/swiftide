//! This module provides integration with `OpenRouter`'s API, enabling the use of language models
//! and embeddings within the Swiftide project. It includes the `OpenRouter` struct for managing API
//! clients and default options for embedding and prompt models. The module is conditionally
//! compiled based on the "openrouter" feature flag.

use config::OpenRouterConfig;

use crate::openai;

pub mod config;

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
///
/// With `use_responses_api(true)`, plaintext reasoning history is replayed by default. Other
/// compatible providers using [`openai::OpenAI`] can opt in with
/// [`Options::replay_plaintext_reasoning`]. This does not enable reasoning fields in Chat
/// Completions.
pub type OpenRouter = openai::GenericOpenAI<OpenRouterConfig>;
pub type OpenRouterBuilder = openai::GenericOpenAIBuilder<OpenRouterConfig>;
pub type OpenRouterBuilderError = openai::GenericOpenAIBuilderError;
pub use openai::{Options, OptionsBuilder, OptionsBuilderError};

impl OpenRouter {
    /// Creates a new `OpenRouterBuilder` for constructing `OpenRouter` instances.
    pub fn builder() -> OpenRouterBuilder {
        let mut builder = OpenRouterBuilder::default();
        builder.default_options(Options::builder().replay_plaintext_reasoning(true));
        builder
    }
}

impl Default for OpenRouter {
    fn default() -> Self {
        Self::builder().build().unwrap()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn plaintext_reasoning_replay_default_survives_options_merge() {
        let client = OpenRouter::builder()
            .default_options(Options::builder().prompt_model("qwen/qwen3-30b-a3b"))
            .build()
            .unwrap();
        assert_eq!(client.options().replay_plaintext_reasoning, Some(true));

        let client = OpenRouter::builder()
            .default_options(Options::builder().replay_plaintext_reasoning(false))
            .build()
            .unwrap();
        assert_eq!(client.options().replay_plaintext_reasoning, Some(false));
    }

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
