use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub(crate) struct AnthropicRequest {
    pub(crate) anthropic_version: &'static str, // always 'bedrock-2023-05-31'
    pub(crate) max_tokens: i32,                 // differs per model
    pub(crate) messages: Vec<AnthropicMessage>,

    // Optional fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) top_k: Option<i32>,
}

#[derive(Serialize)]
pub(crate) struct AnthropicMessage {
    pub(crate) role: &'static str, // 'user' or 'assistant'
    pub(crate) content: Vec<AnthropicMessageContent>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct AnthropicMessageContent {
    #[serde(rename = "type")]
    pub(crate) _type: String, // 'text' or 'image'
    pub(crate) text: String,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct AnthropicResponse {
    pub(crate) id: String,
    pub(crate) model: String,
    #[serde(rename = "type")]
    pub(crate) _type: String,
    pub(crate) role: String,
    pub(crate) content: Vec<AnthropicMessageContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) stop_sequence: Option<String>,

    pub(crate) usage: AnthropicUsage,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct AnthropicUsage {
    pub(crate) input_tokens: i32,
    pub(crate) output_tokens: i32,
}
