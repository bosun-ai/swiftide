#![allow(dead_code)]
use std::sync::Arc;

use aws_sdk_bedrockruntime::Client;
use derive_builder::Builder;
use tokio::runtime::Handle;

mod models;
mod simple_prompt;

// TODO:
// - [ ] Implement the major available models, this is really just titan
//

#[derive(Debug, Builder)]
#[builder(setter(strip_option))]
pub struct AwsBedrock {
    #[builder(setter(into))]
    model_id: String,
    #[builder(default)]
    sdk_config: Option<aws_config::SdkConfig>,
    #[builder(default = "self.default_client()", setter(custom))]
    client: Arc<Client>,
    #[builder(default)]
    model_config: ModelConfig,
    model_family: ModelFamily,
}

impl Clone for AwsBedrock {
    fn clone(&self) -> Self {
        Self {
            model_id: self.model_id.clone(),
            sdk_config: None,
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

    pub fn build_titan_family(model_id: impl Into<String>) -> AwsBedrockBuilder {
        Self::builder().titan().model_id(model_id).to_owned()
    }

    pub fn build_anthropic_family(model_id: impl Into<String>) -> AwsBedrockBuilder {
        Self::builder().anthropic().model_id(model_id).to_owned()
    }
}
impl AwsBedrockBuilder {
    pub fn anthropic(&mut self) -> &mut Self {
        self.model_family = Some(ModelFamily::Anthropic);
        self
    }

    pub fn titan(&mut self) -> &mut Self {
        self.model_family = Some(ModelFamily::Titan);
        self
    }

    fn default_config(&self) -> aws_config::SdkConfig {
        tokio::task::block_in_place(|| {
            Handle::current().block_on(async { aws_config::from_env().load().await })
        })
    }
    fn default_client(&self) -> Arc<Client> {
        match &self.sdk_config {
            Some(Some(config)) => Arc::new(Client::new(config)),
            _ => Arc::new(Client::new(&self.default_config())),
        }
    }

    pub fn client(&mut self, client: Client) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }
}

use serde::{Deserialize, Serialize};

use self::models::ModelFamily;

#[derive(Deserialize, Serialize)]
struct Prompt {
    prompt: String,
}

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
