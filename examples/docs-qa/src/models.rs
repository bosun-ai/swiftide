//! The models used across indexing, querying and evaluation: local `FastEmbed` embedders, and
//! chat models behind any OpenAI-compatible API.

use anyhow::{Context as _, Result};
use swiftide::{
    indexing::{BackoffConfiguration, LanguageModelWithBackOff},
    integrations::{
        fastembed::FastEmbed,
        openai::{OpenAI, OpenAIConfig},
    },
};

/// Local `FastEmbed` models: dense (BGE small) and sparse (SPLADE) embeddings.
///
/// Both indexing and querying must use the same pair, so they are always created together.
#[derive(Clone)]
pub struct Embedders {
    pub dense: FastEmbed,
    pub sparse: FastEmbed,
}

impl Embedders {
    /// Loads both models; downloads them on first use.
    pub fn load() -> Result<Self> {
        Ok(Self {
            dense: FastEmbed::try_default()?,
            sparse: FastEmbed::try_default_sparse()?,
        })
    }
}

// ANCHOR: llm
const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// A chat model that retries rate limits and other transient errors with exponential backoff.
/// Indexing makes two calls per chunk, concurrently, so it will hit rate limits.
pub type RetryingModel = LanguageModelWithBackOff<OpenAI>;

/// An OpenAI-compatible API: `OpenAI` itself, or any gateway or local server that speaks it.
#[derive(Debug, Clone)]
pub struct LlmEndpoint {
    config: OpenAIConfig,
    use_responses_api: bool,
}

impl LlmEndpoint {
    /// Reads `OPENAI_BASE_URL` (optional, defaults to `OpenAI`) and `OPENAI_API_KEY` (required).
    pub fn from_env(use_responses_api: bool) -> Result<Self> {
        let api_key = non_empty_env("OPENAI_API_KEY")
            .context("Set OPENAI_API_KEY to the API key of your OpenAI-compatible endpoint")?;
        let base_url =
            non_empty_env("OPENAI_BASE_URL").unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

        Ok(Self {
            config: OpenAIConfig::new()
                .with_api_base(base_url)
                .with_api_key(api_key),
            use_responses_api,
        })
    }

    /// A client for `model` on this endpoint.
    pub fn model(&self, model: &str) -> Result<OpenAI> {
        Ok(OpenAI::builder()
            .client(async_openai::Client::with_config(self.config.clone()))
            .default_prompt_model(model)
            .use_responses_api(self.use_responses_api)
            .build()?)
    }

    /// The same client, wrapped in Swiftide's backoff decorator. An unreachable endpoint gives
    /// up after two minutes.
    pub fn retrying_model(&self, model: &str) -> Result<RetryingModel> {
        let backoff = BackoffConfiguration {
            initial_interval_sec: 5,
            max_elapsed_time_sec: 120,
            ..BackoffConfiguration::default()
        };
        Ok(LanguageModelWithBackOff::new(self.model(model)?, backoff))
    }
}
// ANCHOR_END: llm

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
}
