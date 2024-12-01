//! An integration with the AWS Bedrock service.
//!
//! Supports various model families for prompting.
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_bedrockruntime::{error::SdkError, primitives::Blob, Client};
use derive_builder::Builder;
use serde::Serialize;
use tokio::runtime::Handle;

#[cfg(test)]
use mockall::{automock, predicate::*};

mod models;
mod simple_prompt;

/// An integration with the AWS Bedrock service.
///
/// Can be used as `SimplePrompt`.
///
/// To use Bedrock, you need to have a model id and access to the service.
/// By default, the aws sdk will be configured from the environment.
/// If you have the aws cli properly configured with a region set, it should work out of the box.
///
/// Otherwise, you can use the builder for customization.
///
/// See the aws cli documentation for more information on how to get access to the service.
#[derive(Debug, Builder)]
#[builder(setter(strip_option))]
pub struct AwsBedrock {
    #[builder(setter(into))]
    /// The model id or arn of the model to use
    model_id: String,

    #[builder(default = self.default_client(), setter(custom))]
    /// The bedrock runtime client
    client: Arc<dyn BedrockPrompt>,
    #[builder(default)]
    /// The model configuration to use
    model_config: ModelConfig,
    /// The model family to use. In bedrock, families share their api.
    model_family: ModelFamily,
}

#[cfg_attr(test, automock)]
#[async_trait]
trait BedrockPrompt: std::fmt::Debug + Send + Sync {
    async fn prompt_u8(&self, model_id: &str, blob: Blob) -> Result<Vec<u8>>;
}

#[async_trait]
impl BedrockPrompt for Client {
    async fn prompt_u8(&self, model_id: &str, blob: Blob) -> Result<Vec<u8>> {
        let response = self
            .invoke_model()
            .body(blob)
            .model_id(model_id)
            .send()
            .await
            .map_err(SdkError::into_service_error)?;

        Ok(response.body.into_inner())
    }
}

impl Clone for AwsBedrock {
    fn clone(&self) -> Self {
        Self {
            model_id: self.model_id.clone(),
            client: self.client.clone(),
            model_config: self.model_config.clone(),
            model_family: self.model_family.clone(),
        }
    }
}

impl AwsBedrock {
    pub fn builder() -> AwsBedrockBuilder {
        AwsBedrockBuilder::default()
    }

    /// Build a new `AwsBedrock` instance with the Titan model family
    pub fn build_titan_family(model_id: impl Into<String>) -> AwsBedrockBuilder {
        Self::builder().titan().model_id(model_id).to_owned()
    }

    /// Build a new `AwsBedrock` instance with the Anthropic model family
    pub fn build_anthropic_family(model_id: impl Into<String>) -> AwsBedrockBuilder {
        Self::builder().anthropic().model_id(model_id).to_owned()
    }
}
impl AwsBedrockBuilder {
    /// Set the model family to Anthropic
    pub fn anthropic(&mut self) -> &mut Self {
        self.model_family = Some(ModelFamily::Anthropic);
        self
    }

    /// Set the model family to Titan
    pub fn titan(&mut self) -> &mut Self {
        self.model_family = Some(ModelFamily::Titan);
        self
    }

    #[allow(clippy::unused_self)]
    fn default_config(&self) -> aws_config::SdkConfig {
        tokio::task::block_in_place(|| {
            Handle::current().block_on(async { aws_config::from_env().load().await })
        })
    }
    fn default_client(&self) -> Arc<Client> {
        Arc::new(Client::new(&self.default_config()))
    }

    /// Set the aws bedrock runtime client
    pub fn client(&mut self, client: Client) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }

    #[cfg(test)]
    #[allow(private_bounds)]
    pub fn test_client(&mut self, client: impl BedrockPrompt + 'static) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }
}

use self::models::ModelFamily;

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfig {
    temperature: f32,
    top_p: f32,
    max_token_count: i32,
    stop_sequences: Vec<String>,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            temperature: 0.5,
            top_p: 0.9,
            max_token_count: 8192,
            stop_sequences: vec![],
        }
    }
}
