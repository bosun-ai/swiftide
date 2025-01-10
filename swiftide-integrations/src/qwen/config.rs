use reqwest::header::{HeaderMap, AUTHORIZATION};
use secrecy::{ExposeSecret as _, Secret};
use serde::Deserialize;

const QWEN_API_BASE: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct QwenConfig {
    api_base: String,
    api_key: Secret<String>,
}

impl Default for QwenConfig {
    fn default() -> Self {
        Self {
            api_base: QWEN_API_BASE.to_string(),
            api_key: get_api_key().into(),
        }
    }
}

fn get_api_key() -> String {
    std::env::var("QWEN_API_KEY")
        .unwrap_or_else(|_| std::env::var("DASHSCOPE_API_KEY").unwrap_or_default())
}

impl async_openai::config::Config for QwenConfig {
    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();

        headers.insert(
            AUTHORIZATION,
            format!("Bearer {}", self.api_key.expose_secret())
                .as_str()
                .parse()
                .unwrap(),
        );

        headers
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.api_base, path)
    }

    fn api_base(&self) -> &str {
        &self.api_base
    }

    fn api_key(&self) -> &Secret<String> {
        &self.api_key
    }

    fn query(&self) -> Vec<(&str, &str)> {
        vec![]
    }
}
