//! This module provides integration with `OpenAI`'s API, enabling the use of language models and embeddings within the Swiftide project.
//! It includes the `OpenAI` struct for managing API clients and default options for embedding and prompt models.
//! The module is conditionally compiled based on the "openai" feature flag.

use async_openai::error::OpenAIError;
use derive_builder::Builder;
use std::sync::Arc;
use swiftide_core::chat_completion::errors::ChatCompletionError;

mod chat_completion;
mod embed;
mod simple_prompt;

// expose type aliases to simplify downstream use of the open ai builder invocations
pub use async_openai::config::AzureConfig;
pub use async_openai::config::OpenAIConfig;

/// The `OpenAI` struct encapsulates an `OpenAI` client and default options for embedding and prompt models.
/// It uses the `Builder` pattern for flexible and customizable instantiation.
///
/// # Example
///
/// ```no_run
/// # use swiftide_integrations::openai::OpenAI;
/// # use swiftide_integrations::openai::OpenAIConfig;
///
/// // Create an OpenAI client with default options. The client will use the OPENAI_API_KEY environment variable.
/// let openai = OpenAI::<OpenAIConfig>::builder()
///     .default_embed_model("text-embedding-3-small")
///     .default_prompt_model("gpt-4")
///     .build().unwrap();
///
/// // Create an OpenAI client with a custom api key.
/// let openai = OpenAI::<OpenAIConfig>::builder()
///     .default_embed_model("text-embedding-3-small")
///     .default_prompt_model("gpt-4")
///     .client(async_openai::Client::with_config(async_openai::config::OpenAIConfig::default().with_api_key("my-api-key")))
///     .build().unwrap();
///```
#[derive(Debug, Builder, Clone)]
#[builder(setter(into, strip_option))]
pub struct OpenAI<C: async_openai::config::Config + Default = async_openai::config::OpenAIConfig> {
    /// The `OpenAI` client, wrapped in an `Arc` for thread-safe reference counting.
    /// Defaults to a new instance of `async_openai::Client`.
    #[builder(
        default = "Arc::new(async_openai::Client::<C>::default())",
        setter(custom)
    )]
    client: Arc<async_openai::Client<C>>,

    /// Default options for embedding and prompt models.
    #[builder(default)]
    default_options: Options,
}

/// The `Options` struct holds configuration options for the `OpenAI` client.
/// It includes optional fields for specifying the embedding and prompt models.
#[derive(Debug, Clone, Builder)]
#[builder(setter(into, strip_option))]
pub struct Options {
    /// The default embedding model to use, if specified.
    #[builder(default)]
    pub embed_model: Option<String>,
    /// The default prompt model to use, if specified.
    #[builder(default)]
    pub prompt_model: Option<String>,

    #[builder(default = true)]
    /// Option to enable or disable parallel tool calls for completions.
    ///
    /// At this moment, o1 and o3-mini do not support it.
    pub parallel_tool_calls: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            embed_model: None,
            prompt_model: None,
            parallel_tool_calls: true,
        }
    }
}

impl Options {
    /// Creates a new `OptionsBuilder` for constructing `Options` instances.
    pub fn builder() -> OptionsBuilder {
        OptionsBuilder::default()
    }
}

impl OpenAI<OpenAIConfig> {
    /// Creates a new `OpenAIBuilder` for constructing `OpenAI` instances.
    pub fn builder() -> OpenAIBuilder<OpenAIConfig> {
        OpenAIBuilder::<OpenAIConfig>::default()
    }
}

impl<C: async_openai::config::Config + Default + Sync + Send + std::fmt::Debug> OpenAIBuilder<C> {
    /// Sets the `OpenAI` client for the `OpenAI` instance.
    ///
    /// # Parameters
    /// - `client`: The `OpenAI` client to set.
    ///
    /// # Returns
    /// A mutable reference to the `OpenAIBuilder`.
    pub fn client(&mut self, client: async_openai::Client<C>) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }

    /// Sets the default embedding model for the `OpenAI` instance.
    ///
    /// # Parameters
    /// - `model`: The embedding model to set.
    ///
    /// # Returns
    /// A mutable reference to the `OpenAIBuilder`.
    pub fn default_embed_model(&mut self, model: impl Into<String>) -> &mut Self {
        if let Some(options) = self.default_options.as_mut() {
            options.embed_model = Some(model.into());
        } else {
            self.default_options = Some(Options {
                embed_model: Some(model.into()),
                ..Default::default()
            });
        }
        self
    }

    /// Enable or disable parallel tool calls for completions.
    ///
    /// Note that currently reasoning models do not support parallel tool calls
    ///
    /// Defaults to `true`
    pub fn parallel_tool_calls(&mut self, parallel_tool_calls: bool) -> &mut Self {
        if let Some(options) = self.default_options.as_mut() {
            options.parallel_tool_calls = parallel_tool_calls;
        } else {
            self.default_options = Some(Options {
                parallel_tool_calls,
                ..Default::default()
            });
        }
        self
    }

    /// Sets the default prompt model for the `OpenAI` instance.
    ///
    /// # Parameters
    /// - `model`: The prompt model to set.
    ///
    /// # Returns
    /// A mutable reference to the `OpenAIBuilder`.
    pub fn default_prompt_model(&mut self, model: impl Into<String>) -> &mut Self {
        if let Some(options) = self.default_options.as_mut() {
            options.prompt_model = Some(model.into());
        } else {
            self.default_options = Some(Options {
                prompt_model: Some(model.into()),
                ..Default::default()
            });
        }
        self
    }
}

pub fn open_ai_error_to_completion_error(e: OpenAIError) -> ChatCompletionError {
    match e {
        OpenAIError::ApiError(api_error) => {
            // If the response is an ApiError, it could be a context length exceeded error
            if api_error.code == Some("context_length_exceeded".to_string()) {
                ChatCompletionError::ContextLengthExceeded(OpenAIError::ApiError(api_error).into())
            } else {
                tracing::error!("OpenAI API Error: {:?}", api_error);
                ChatCompletionError::ClientError(OpenAIError::ApiError(api_error).into())
            }
        }
        OpenAIError::Reqwest(e) => match e.status() {
            Some(status) => {
                // If the response code is 429 it could either be a TransientError or a ClientError depending
                // on the message, if it contains the word quota, it should be a ClientError otherwise it should
                // be a TransientError.
                // If the response code is any other 4xx it should be a ClientError.
                if status.as_u16() == 429 && !e.to_string().contains("quota") {
                    ChatCompletionError::TransientError(e.into())
                } else if status.is_client_error() {
                    tracing::error!("OpenAI API Client Error: {:?}", e);
                    ChatCompletionError::ClientError(e.into())
                } else if status.is_server_error() {
                    tracing::warn!("OpenAI API Server Error: {:?}", e);
                    ChatCompletionError::TransientError(e.into())
                } else {
                    tracing::error!("Unexpected OpenAI Error: {:?}, error: {:?}", status, e);
                    ChatCompletionError::ClientError(e.into())
                }
            }
            _ => {
                // making the request failed for some other reason, probably recoverable
                tracing::error!("Unexpected OpenAI Reqwest Error: {:?}", e);
                ChatCompletionError::TransientError(e.into())
            }
        },
        OpenAIError::JSONDeserialize(e) => {
            // OpenAI generated a non-json response, probably a temporary problem on their side
            tracing::error!("OpenAI response could not be deserialized: {:?}", e);
            ChatCompletionError::TransientError(e.into())
        }
        OpenAIError::FileSaveError(msg) => {
            tracing::error!("OpenAI Failed to save file: {:?}", msg);
            ChatCompletionError::ClientError(OpenAIError::FileSaveError(msg).into())
        }
        OpenAIError::FileReadError(msg) => {
            tracing::error!("OpenAI Failed to read file: {:?}", msg);
            ChatCompletionError::ClientError(OpenAIError::FileReadError(msg).into())
        }
        OpenAIError::StreamError(msg) => {
            tracing::error!("OpenAI Stream failed: {:?}", msg);
            ChatCompletionError::ClientError(OpenAIError::StreamError(msg).into())
        }
        OpenAIError::InvalidArgument(msg) => {
            tracing::error!("OpenAI Invalid Argument: {:?}", msg);
            ChatCompletionError::ClientError(OpenAIError::InvalidArgument(msg).into())
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// test default embed model
    #[test]
    fn test_default_embed_and_prompt_model() {
        let openai: OpenAI<async_openai::config::OpenAIConfig> = OpenAI::builder()
            .default_embed_model("gpt-3")
            .default_prompt_model("gpt-4")
            .build()
            .unwrap();
        assert_eq!(
            openai.default_options.embed_model,
            Some("gpt-3".to_string())
        );
        assert_eq!(
            openai.default_options.prompt_model,
            Some("gpt-4".to_string())
        );

        let openai: OpenAI<async_openai::config::OpenAIConfig> = OpenAI::builder()
            .default_prompt_model("gpt-4")
            .default_embed_model("gpt-3")
            .build()
            .unwrap();
        assert_eq!(
            openai.default_options.prompt_model,
            Some("gpt-4".to_string())
        );
        assert_eq!(
            openai.default_options.embed_model,
            Some("gpt-3".to_string())
        );
    }
}
