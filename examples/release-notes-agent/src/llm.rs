//! The language model: any `OpenAI`-compatible endpoint, configured from the environment.
use std::{env, fmt};

use anyhow::{Result, anyhow};
use swiftide::integrations::openai::OpenAI;

/// The API used when `OPENAI_BASE_URL` is not set.
const OPENAI_API: &str = "https://api.openai.com/v1";

/// Which `OpenAI` API the completions are sent to.
#[derive(Debug, Clone, Copy)]
pub enum OpenAiApi {
    ChatCompletions,
    Responses,
}

impl OpenAiApi {
    /// From the `--responses-api` flag.
    pub fn from_flag(responses_api: bool) -> Self {
        if responses_api {
            Self::Responses
        } else {
            Self::ChatCompletions
        }
    }
}

impl fmt::Display for OpenAiApi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ChatCompletions => "chat completions",
            Self::Responses => "Responses API",
        })
    }
}

/// Where completions are sent, and how.
pub struct LlmEndpoint {
    /// An `OpenAI`-compatible API, from `OPENAI_BASE_URL`.
    base_url: String,
    /// From `OPENAI_API_KEY`.
    api_key: String,
    model: String,
    api: OpenAiApi,
}

impl LlmEndpoint {
    /// Reads the endpoint from `OPENAI_BASE_URL` (optional) and `OPENAI_API_KEY` (required).
    ///
    /// Empty values, such as `OPENAI_API_KEY=` in `.env`, count as unset.
    pub fn from_env(model: String, api: OpenAiApi) -> Result<Self> {
        // The client would only find out on the first request, with a long 401.
        let api_key = non_empty_var("OPENAI_API_KEY").ok_or_else(|| {
            anyhow!("OPENAI_API_KEY is not set; export it, add it to ./.env, or pass --env-file")
        })?;
        let base_url = non_empty_var("OPENAI_BASE_URL").unwrap_or_else(|| OPENAI_API.to_owned());
        Ok(Self {
            base_url,
            api_key,
            model,
            api,
        })
    }

    /// An `OpenAI` client that streams deltas only, rather than the accumulated message.
    pub fn client(&self) -> Result<OpenAI> {
        Ok(OpenAI::builder()
            .api_base(&self.base_url)
            .api_key(&self.api_key)
            .default_prompt_model(&self.model)
            .use_responses_api(matches!(self.api, OpenAiApi::Responses))
            .stream_full(false)
            .build()?)
    }
}

/// `claude-sonnet-5-5 at http://127.0.0.1:8080/v1 (chat completions)`
impl fmt::Display for LlmEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {} ({})", self.model, self.base_url, self.api)
    }
}

/// The variable's value, unless it is unset or blank.
fn non_empty_var(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}
