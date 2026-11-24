//! The language model used to describe chunks and to answer questions.
//!
//! Any OpenAI-compatible API works (`OpenAI`, a gateway in front of another provider, a local
//! server, ...): the client is always swiftide's `OpenAI` integration, pointed at a different
//! base URL.

use std::{fmt, sync::Arc, time::Duration};

use anyhow::{Context as _, Result};
use swiftide::{
    indexing::{BackoffConfiguration, LanguageModelWithBackOff},
    integrations::openai::OpenAI,
    traits::SimplePrompt,
};

/// Environment variable with the base URL of an OpenAI-compatible API.
const BASE_URL_ENV: &str = "OPENAI_BASE_URL";
const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
/// Environment variable with the API key for that API.
const API_KEY_ENV: &str = "OPENAI_API_KEY";

/// Which API the prompts are sent to.
#[derive(Debug, Clone, Copy)]
pub enum OpenAiApi {
    ChatCompletions,
    Responses,
}

/// Where to send prompts and which model to use.
#[derive(Debug, Clone)]
pub struct LlmEndpoint {
    /// Base URL of an OpenAI-compatible API, e.g. `https://api.openai.com/v1`.
    base_url: String,
    api_key: String,
    model: String,
    api: OpenAiApi,
}

impl LlmEndpoint {
    /// Reads the base URL (`OPENAI_BASE_URL`, optional) and API key (`OPENAI_API_KEY`) from the
    /// environment.
    ///
    /// Empty values count as unset.
    pub fn from_env(model: String, api: OpenAiApi) -> Result<Self> {
        let base_url = non_empty_var(BASE_URL_ENV).unwrap_or_else(|| DEFAULT_BASE_URL.to_owned());
        let api_key = non_empty_var(API_KEY_ENV).with_context(|| {
            format!(
                "{API_KEY_ENV} is not set; export the API key of your OpenAI-compatible endpoint"
            )
        })?;

        Ok(Self {
            base_url,
            api_key,
            model,
            api,
        })
    }
}

/// Names the endpoint (never the key), so errors can point at a wrong URL or model.
impl fmt::Display for LlmEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "LLM endpoint {} ({BASE_URL_ENV}), model {}",
            self.base_url, self.model
        )
    }
}

fn non_empty_var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

// ANCHOR: llm
impl LlmEndpoint {
    /// Builds a client for this endpoint.
    ///
    /// Rate limits and transient errors are retried with exponential backoff, which matters when
    /// indexing fires many prompts concurrently. After `retry_budget` the error is returned, so
    /// an unreachable or broken endpoint fails instead of hanging.
    pub fn client(&self, retry_budget: Duration) -> Result<Arc<dyn SimplePrompt>> {
        let openai = OpenAI::builder()
            .api_base(&self.base_url)
            .api_key(&self.api_key)
            .default_prompt_model(&self.model)
            .use_responses_api(matches!(self.api, OpenAiApi::Responses))
            .build()?;

        let backoff = BackoffConfiguration {
            max_elapsed_time_sec: retry_budget.as_secs(),
            ..BackoffConfiguration::default()
        };

        Ok(Arc::new(LanguageModelWithBackOff::new(openai, backoff)))
    }
}
// ANCHOR_END: llm
