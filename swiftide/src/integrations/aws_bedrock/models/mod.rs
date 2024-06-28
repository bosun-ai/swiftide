use anyhow::{Context as _, Result};
use itertools::Itertools;

use super::ModelConfig;

pub mod anthropic;
pub mod titan;

pub(crate) use anthropic::*;
pub(crate) use titan::*;

#[derive(Clone, Debug)]
/// The model family to use for bedrock
///
/// This enum is used to determine which model family to use when sending a request to bedrock.
///
/// A model id or arn and access is required to use the bedrock api.
pub enum ModelFamily {
    Anthropic,
    Titan,
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
                let mut response: TitanResponse =
                    serde_json::from_slice(response_bytes).context("Failed to parse response")?;

                if response.results.is_empty() {
                    return Err(anyhow::anyhow!("No results returned"));
                } else {
                    Ok(response.results.swap_remove(0).output_text)
                }
            }
        }
    }
}
