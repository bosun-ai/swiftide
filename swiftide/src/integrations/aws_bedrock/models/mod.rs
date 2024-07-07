use anyhow::{Context as _, Result};

use super::ModelConfig;

pub mod anthropic;
pub mod titan;

pub(crate) use anthropic::*;
pub(crate) use titan::*;

#[derive(Clone, Debug)]
/// The model family to use for bedrock
///
/// A model id or arn and access is required to use the bedrock api.
pub enum ModelFamily {
    /// The anthropic model family, only the newer messaging API is supported
    Anthropic,
    /// The titan model family
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
            ModelFamily::Anthropic => {
                let request = AnthropicRequest {
                    anthropic_version: "bedrock-2023-05-31",
                    max_tokens: model_config.max_token_count,
                    messages: vec![AnthropicMessage {
                        role: "user",
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
                let mut response: AnthropicResponse =
                    serde_json::from_slice(response_bytes).context("Failed to parse response")?;

                if response.content.is_empty() {
                    Err(anyhow::anyhow!("No results returned"))
                } else {
                    Ok(response.content.swap_remove(0).text)
                }
            }
            ModelFamily::Titan => {
                let mut response: TitanResponse =
                    serde_json::from_slice(response_bytes).context("Failed to parse response")?;

                if response.results.is_empty() {
                    return Err(anyhow::anyhow!("No results returned"));
                }

                Ok(response.results.swap_remove(0).output_text)
            }
        }
    }
}
