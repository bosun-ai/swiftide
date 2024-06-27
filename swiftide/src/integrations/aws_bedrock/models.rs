use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

use super::ModelConfig;

#[derive(Clone, Debug)]
pub enum ModelFamily {
    Anthropic,
    Titan,
    // MetaLlama,
    // Mistral,
}

impl ModelFamily {
    #[tracing::instrument(skip_all)]
    pub(crate) fn build_request_to_bytes(
        &self,
        input_text: impl AsRef<str>,
        model_config: &ModelConfig,
    ) -> Result<Vec<u8>> {
        match self {
            // TODO: Clean up allocations
            ModelFamily::Anthropic => {
                let request = AnthropicRequest {
                    anthropic_version: "bedrock-2023-05-31".to_string(),
                    max_tokens: model_config.max_token_count,
                    messages: vec![AnthropicMessage {
                        role: "user".to_string(),
                        content: vec![AnthropicMessageContent {
                            _type: "text".to_string(),
                            text: input_text.as_ref().to_string(),
                        }],
                    }],
                    system_prompt: None,
                    stop_sequences: None,
                    temperature: Some(model_config.temperature),
                    top_p: None,
                    top_k: None,
                };
                serde_json::to_vec(&request).context("Failed to serialize request")
            }
            ModelFamily::Titan => {
                let request = TitanRequest {
                    input_text: input_text.as_ref().to_string(),
                    text_generation_config: model_config.clone(),
                };
                serde_json::to_vec(&request).context("Failed to serialize request")
            }
        }
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn output_message_from_bytes(&self, response_bytes: &[u8]) -> Result<String> {
        match self {
            ModelFamily::Anthropic => {
                let response: AnthropicResponse =
                    serde_json::from_slice(response_bytes).context("Failed to parse response")?;
                let output_text = response
                    .content
                    .into_iter()
                    .take(1)
                    .filter_map(|content| {
                        if content._type == "text" {
                            Some(content.text)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<String>>()
                    .join("\n");
                Ok(output_text)
            }
            ModelFamily::Titan => {
                let response: TitanResponse =
                    serde_json::from_slice(response_bytes).context("Failed to parse response")?;
                let output_text = response
                    .results
                    .into_iter()
                    .filter_map(|result| {
                        if result.completion_reason == "stop" {
                            Some(result.output_text)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<String>>()
                    .join("\n");
                Ok(output_text)
            }
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TitanRequest {
    input_text: String,
    text_generation_config: ModelConfig,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct TitanResponse {
    input_text_token_count: i32,
    results: Vec<TitanTextResult>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct TitanTextResult {
    token_count: i32,
    output_text: String,
    completion_reason: String,
}

#[derive(Serialize)]
struct AnthropicRequest {
    anthropic_version: String, // always 'bedrock-2023-05-31'
    max_tokens: i32,           // differs per model
    messages: Vec<AnthropicMessage>,

    // Optional fields
    #[serde(skip_serializing_if = "Option::is_none")]
    system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<i32>,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String, // 'user' or 'assistant'
    content: Vec<AnthropicMessageContent>,
}

#[derive(Serialize, Deserialize)]
struct AnthropicMessageContent {
    #[serde(rename = "type")]
    _type: String, // 'text' or 'image'
    text: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    id: String,
    model: String,
    #[serde(rename = "type")]
    _type: String,
    role: String,
    content: Vec<AnthropicMessageContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequence: Option<String>,

    usage: AnthropicUsage,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: i32,
    output_tokens: i32,
}
