//! The OpenAI-compatible endpoint the crate analysts and the document writer run on.
//!
//! Any API that speaks the `OpenAI` protocol works (`OpenAI` itself, a gateway, a local server):
//! the client is always swiftide's `OpenAI` integration, pointed at `OPENAI_BASE_URL`.

use std::fmt;

use anyhow::{Context as _, Result};
use swiftide::integrations::openai::{OpenAI, Options};

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// Output budget per completion, enough for the whole ARCHITECTURE.md in one answer.
///
/// Set explicitly because some OpenAI-compatible gateways cap an unset limit at 4096 tokens, and
/// a `SimplePrompt` answer cut off at the limit looks like a complete one.
const MAX_OUTPUT_TOKENS: u32 = 16_384;

/// The model choices a run makes on the command line.
#[derive(Debug, clap::Args)]
pub struct ModelArgs {
    /// Chat model for the crate analysts and the document writer.
    #[arg(long, env = "OPENAI_MODEL", default_value = "gpt-5.4")]
    pub model: String,

    /// Use the Responses API instead of Chat Completions.
    #[arg(long)]
    pub responses_api: bool,
}

/// The model and the API it is called through, e.g. `gpt-5.4 via Chat Completions`.
impl fmt::Display for ModelArgs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let api = if self.responses_api {
            "Responses API"
        } else {
            "Chat Completions"
        };
        write!(f, "{} via {api}", self.model)
    }
}

/// Where requests go: `OPENAI_BASE_URL` (optional) and `OPENAI_API_KEY` (required).
#[derive(Clone)]
pub struct Endpoint {
    base_url: String,
    api_key: String,
}

impl Endpoint {
    /// Reads the endpoint from the environment and fails before any agent starts without a key.
    pub fn from_env() -> Result<Self> {
        let api_key = non_empty_env("OPENAI_API_KEY")
            .context("OPENAI_API_KEY is not set (export it or put it in ./.env)")?;
        let base_url =
            non_empty_env("OPENAI_BASE_URL").unwrap_or_else(|| DEFAULT_BASE_URL.to_owned());
        Ok(Self { base_url, api_key })
    }

    /// A client for `model` on this endpoint.
    pub fn client(&self, model: &ModelArgs) -> Result<OpenAI> {
        Ok(OpenAI::builder()
            .api_base(&self.base_url)
            .api_key(&self.api_key)
            .default_prompt_model(&model.model)
            .default_options(Options::builder().max_completion_tokens(MAX_OUTPUT_TOKENS))
            .use_responses_api(model.responses_api)
            .build()?)
    }
}

/// An environment variable, treating an empty value (as in a copied `.env` template) as unset.
fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// Shows where requests go, never the key.
impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.base_url)
    }
}

impl fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Endpoint")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}
