#![allow(dead_code)]
use std::sync::Arc;

use aws_sdk_bedrockruntime::Client;
use derive_builder::Builder;
use tokio::runtime::Handle;

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
}

impl Clone for AwsBedrock {
    fn clone(&self) -> Self {
        Self {
            model_id: self.model_id.clone(),
            sdk_config: None,
            client: self.client.clone(),
            model_config: self.model_config.clone(),
        }
    }
}

impl AwsBedrock {
    pub fn builder() -> AwsBedrockBuilder {
        AwsBedrockBuilder::default()
    }
}
impl AwsBedrockBuilder {
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

#[derive(Deserialize, Serialize)]
struct Prompt {
    prompt: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct BedrockResponse {
    input_text_token_count: i32,
    results: Vec<BedrockTextResult>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct BedrockTextResult {
    token_count: i32,
    output_text: String,
    completion_reason: String,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct ModelConfig {
    temperature: f32,
    top_p: f32,
    max_token_count: i32,
    stop_sequences: Vec<String>,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            top_p: 0.9,
            max_token_count: 8192,
            stop_sequences: vec![],
        }
    }
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct BedrockRequest {
    input_text: String,
    text_generation_config: ModelConfig,
}

impl BedrockRequest {
    fn new(prompt: impl Into<String>, config: ModelConfig) -> Self {
        Self {
            input_text: prompt.into(),
            text_generation_config: config,
        }
    }
}
