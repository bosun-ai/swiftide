use std::fmt::Debug;

use anyhow::{Context as _, Result};
use async_openai::types::{
    ChatCompletionRequestMessage, ChatCompletionRequestUserMessageArgs,
    CreateChatCompletionRequestArgs, CreateEmbeddingRequestArgs,
};
use async_trait::async_trait;

mod config;
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4o";
pub const DEFAULT_OPENAI_EMBEDDING_MODEL: &str = "text-embedding-3-small";

use qdrant_client::client::QdrantClient;

// Loads the global config async
pub fn config() -> &'static config::Config {
    config::Config::from_env()
}

pub fn create_openai_client() -> async_openai::Client<async_openai::config::OpenAIConfig> {
    let mut openai_config =
        async_openai::config::OpenAIConfig::new().with_api_key(&config().openai_api_key);

    // Enables mocking in tests
    if let Some(endpoint) = &config().openai_endpoint {
        openai_config = openai_config.with_api_base(endpoint);
    } else if cfg!(feature = "integration_testing") {
        panic!("Openai endpoint not set in testing");
    }
    async_openai::Client::with_config(openai_config)
}

pub fn create_qdrant_client() -> Result<QdrantClient> {
    let url = &config()
        .qdrant_url
        .as_deref()
        .ok_or(anyhow::anyhow!("qdrant url missing from config"))?;

    QdrantClient::from_url(url)
        .with_api_key(config().qdrant_api_key.clone())
        .build()
}

#[async_trait]
pub trait SimplePrompt: Debug + Send + Sync {
    // Takes a simple prompt, prompts the llm and returns the response
    async fn prompt(&self, prompt: &str, model: &str) -> Result<String>;
}

#[async_trait]
pub trait SimpleCompletion<T> {
    async fn complete(&self, messages: Vec<T>, prompt: &str, model: &str) -> Result<String>;
}

#[async_trait]
#[allow(clippy::blocks_in_conditions)]
impl SimplePrompt for async_openai::Client<async_openai::config::OpenAIConfig> {
    #[tracing::instrument(skip(self), err)]
    async fn prompt(&self, prompt: &str, model: &str) -> Result<String> {
        let request = CreateChatCompletionRequestArgs::default()
            .model(model)
            .messages(vec![ChatCompletionRequestUserMessageArgs::default()
                .content(prompt)
                .build()?
                .into()])
            .build()?;

        tracing::debug!(
            messages = serde_json::to_string_pretty(&request)?,
            "[SimplePrompt] Request to openai"
        );

        let mut response = self.chat().create(request).await?;

        tracing::debug!(
            response = serde_json::to_string_pretty(&response)?,
            "[SimplePrompt] Response from openai"
        );

        response
            .choices
            .remove(0)
            .message
            .content
            .take()
            .context("Expected content in response")
    }
}

#[async_trait]
#[allow(clippy::blocks_in_conditions)]
impl SimpleCompletion<ChatCompletionRequestMessage>
    for async_openai::Client<async_openai::config::OpenAIConfig>
{
    // Takes a simple prompt, prompts the llm and returns the response
    #[tracing::instrument(skip(self), err)]
    async fn complete(
        &self,
        messages: Vec<ChatCompletionRequestMessage>,
        prompt: &str,
        model: &str,
    ) -> Result<String> {
        let mut messages = messages.to_vec();
        messages.push(
            ChatCompletionRequestUserMessageArgs::default()
                .content(prompt)
                .build()?
                .into(),
        );

        let request = CreateChatCompletionRequestArgs::default()
            .model(model)
            .messages(messages)
            .build()?;

        tracing::debug!(
            messages = serde_json::to_string_pretty(&request)?,
            "[SimpleCompletion] Request to openai"
        );

        let response = self.chat().create(request).await?;

        tracing::debug!(
            response = serde_json::to_string_pretty(&response)?,
            "[SimpleCompletion] Response from openai"
        );

        Ok(response
            .choices
            .first()
            .unwrap()
            .message
            .content
            .as_ref()
            .expect("Expected content in response")
            .clone())
    }
}

#[async_trait]
impl Embed for async_openai::Client<async_openai::config::OpenAIConfig> {
    // WARN: Openai-async clones the input
    async fn embed(&self, input: Vec<String>, model: &str) -> Result<Embeddings> {
        let request = CreateEmbeddingRequestArgs::default()
            .model(model)
            .input(input)
            .build()?;
        tracing::debug!(
            messages = serde_json::to_string_pretty(&request)?,
            "[Embed] Request to openai"
        );
        let response = self.embeddings().create(request).await?;
        tracing::debug!("[Embed] Response openai");

        Ok(response.data.into_iter().map(|d| d.embedding).collect())
    }
}
