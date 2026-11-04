//! Mistral Chat Completions, independent of Swiftide's `OpenAI` client.
//!
//! Uses async-openai's unchanged HTTP client, request builders, tool and usage types with
//! Mistral-specific response decoding. Thinking stays separate from visible text and is replayed
//! before the assistant message. Swiftide agents retain this history automatically.
//!
//! ```no_run
//! use swiftide_integrations::mistral::{Mistral, Options, ReasoningEffort};
//! let client = Mistral::builder()
//!     .default_prompt_model("mistral-large-4")
//!     .default_options(Options::builder().reasoning_effort(ReasoningEffort::High))
//!     .build().unwrap();
//! ```
//!
//! The default transport uses `MISTRAL_API_KEY` and `https://api.mistral.ai/v1`. A custom
//! async-openai client can be supplied for a different endpoint. Prompt-cache options can be
//! passed through [`OptionsBuilder::extra_body`]; cache hits are controlled by the provider.
//! For normalized Bifrost Mistral responses, select [`ChatFormat::Bifrost`], supply the gateway
//! client and use a `mistral/` model prefix. This supports Mistral plaintext thinking and its
//! optional `closed` flag, not signed/encrypted reasoning formats from unrelated providers.
//! It still requires the separate Bifrost normalization fix. Existing Swiftide `OpenAI`
//! configuration, execution, streaming and error handling are unchanged.
//!
//! ```no_run
//! use swiftide_integrations::mistral::{ChatFormat, Mistral, OpenAIConfig};
//! let gateway = async_openai::Client::with_config(OpenAIConfig::new()
//!     .with_api_base("http://localhost:8080/v1")
//!     .with_api_key(std::env::var("BIFROST_API_KEY").unwrap()));
//! let client = Mistral::builder()
//!     .client(gateway)
//!     .chat_format(ChatFormat::Bifrost)
//!     .default_prompt_model("mistral/mistral-large-4")
//!     .build().unwrap();
//! ```
//!
//! Run `cargo run -p swiftide-integrations --features mistral --example mistral -- --stream`
//! for a two-turn tool/reasoning example (makes paid requests).

use async_openai::types::chat::CreateChatCompletionRequestArgs;
use derive_builder::Builder;
use std::{pin::Pin, sync::Arc};
use swiftide_core::chat_completion::Usage;

pub use async_openai::config::OpenAIConfig;
pub use async_openai::types::responses::ReasoningEffort;

mod bifrost;
#[cfg(test)]
mod bifrost_tests;
mod chat_completion;
mod error;
#[cfg(test)]
mod error_tests;
mod messages;
mod simple_prompt;
mod structured_prompt;
#[cfg(test)]
mod tests;
mod types;
mod wire;

type UsageCallback = Arc<
    dyn for<'a> Fn(
            &'a Usage,
        )
            -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>>
        + Send
        + Sync,
>;

/// Select the Mistral endpoint's wire format explicitly; never inferred from model names.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChatFormat {
    #[default]
    Native,
    /// Bifrost's normalized Mistral text/thinking format. Requires its Mistral normalization
    /// fix; use a gateway URL/key and `mistral/` model prefix.
    Bifrost,
}

/// Native Mistral client. It implements chat, streaming, simple and structured prompting.
#[derive(Clone, Builder)]
#[builder(setter(into, strip_option))]
pub struct Mistral {
    #[builder(default)]
    chat_format: ChatFormat,
    #[builder(default = "default_client()", setter(custom))]
    client: Arc<async_openai::Client<OpenAIConfig>>,
    #[builder(default, setter(custom))]
    default_options: Options,
    /// Include accumulated text/reasoning/tools on intermediate stream events. The final
    /// snapshot always contains the complete result, with no new delta.
    #[builder(default = true)]
    pub stream_full: bool,
    #[builder(default, setter(custom))]
    on_usage: Option<UsageCallback>,
    #[cfg(feature = "metrics")]
    #[builder(default)]
    metric_metadata: Option<std::collections::HashMap<String, String>>,
}

impl std::fmt::Debug for Mistral {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mistral")
            .field("model", &self.default_options.prompt_model)
            .field("stream_full", &self.stream_full)
            .finish_non_exhaustive()
    }
}

fn default_client() -> Arc<async_openai::Client<OpenAIConfig>> {
    Arc::new(async_openai::Client::with_config(
        OpenAIConfig::new()
            .with_api_base("https://api.mistral.ai/v1")
            .with_api_key(std::env::var("MISTRAL_API_KEY").unwrap_or_default()),
    ))
}

/// Mistral request options. Reasoning, tools and token accounting do not alter `OpenAI` options.
#[derive(Debug, Clone, Default, Builder)]
#[builder(default, setter(strip_option))]
pub struct Options {
    #[builder(setter(into))]
    pub prompt_model: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub random_seed: Option<i64>,
    pub parallel_tool_calls: Option<bool>,
    pub reasoning_effort: Option<ReasoningEffort>,
    /// Additional provider options, such as `prompt_cache_key`. Applied after defaults.
    pub extra_body: serde_json::Map<String, serde_json::Value>,
}

impl Options {
    pub fn builder() -> OptionsBuilder {
        OptionsBuilder::default()
    }
}

impl From<OptionsBuilder> for Options {
    fn from(value: OptionsBuilder) -> Self {
        value.build().expect("all options have defaults")
    }
}

impl From<&mut OptionsBuilder> for Options {
    fn from(value: &mut OptionsBuilder) -> Self {
        value.build().expect("all options have defaults")
    }
}

impl MistralBuilder {
    pub fn client(&mut self, client: async_openai::Client<OpenAIConfig>) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }

    pub fn default_prompt_model(&mut self, model: impl Into<String>) -> &mut Self {
        self.default_options
            .get_or_insert_with(Options::default)
            .prompt_model = Some(model.into());
        self
    }

    /// Replace request options, retaining the configured model when the new options omit it.
    pub fn default_options(&mut self, options: impl Into<Options>) -> &mut Self {
        let mut options = options.into();
        if options.prompt_model.is_none() {
            options.prompt_model = self
                .default_options
                .as_ref()
                .and_then(|old| old.prompt_model.clone());
        }
        self.default_options = Some(options);
        self
    }

    /// Report provider usage once on completion or failure, when available. This callback runs
    /// in a spawned task and cannot enforce a hard spending limit; callers must bound retries.
    pub fn on_usage<F>(&mut self, func: F) -> &mut Self
    where
        F: Fn(&Usage) -> anyhow::Result<()> + Send + Sync + 'static,
    {
        self.on_usage_async(move |usage| {
            let result = func(usage);
            Box::pin(async move { result })
        })
    }

    pub fn on_usage_async<F>(&mut self, func: F) -> &mut Self
    where
        F: for<'a> Fn(
                &'a Usage,
            )
                -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>>
            + Send
            + Sync
            + 'static,
    {
        self.on_usage = Some(Some(Arc::new(func)));
        self
    }
}

impl Mistral {
    pub fn builder() -> MistralBuilder {
        MistralBuilder::default()
    }
    pub fn client(&self) -> &Arc<async_openai::Client<OpenAIConfig>> {
        &self.client
    }
    pub fn options(&self) -> &Options {
        &self.default_options
    }
    pub fn options_mut(&mut self) -> &mut Options {
        &mut self.default_options
    }

    fn request_defaults(&self) -> CreateChatCompletionRequestArgs {
        let options = &self.default_options;
        let mut args = CreateChatCompletionRequestArgs::default();
        if let Some(value) = options.max_tokens {
            args.max_completion_tokens(value);
        }
        if let Some(value) = options.temperature {
            args.temperature(value);
        }
        if let Some(value) = options.top_p {
            args.top_p(value);
        }
        if let Some(value) = options.random_seed {
            args.seed(value);
        }
        if let Some(value) = options.parallel_tool_calls {
            args.parallel_tool_calls(value);
        }
        if let Some(value) = &options.reasoning_effort {
            args.reasoning_effort(value.clone());
        }
        args
    }
}
