use super::ModelConfig;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TitanRequest {
    pub(crate) input_text: String,
    pub(crate) text_generation_config: ModelConfig,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TitanResponse {
    pub(crate) input_text_token_count: i32,
    pub(crate) results: Vec<TitanTextResult>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TitanTextResult {
    pub(crate) token_count: i32,
    pub(crate) output_text: String,
    pub(crate) completion_reason: String,
}
