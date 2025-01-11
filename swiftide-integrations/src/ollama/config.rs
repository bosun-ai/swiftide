use reqwest::header::HeaderMap;
use secrecy::Secret;
use serde::Deserialize;

const OLLAMA_API_BASE: &str = "http://localhost:11434";

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct OllamaConfig {
    api_base: String,
    api_key: Secret<String>,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            api_base: OLLAMA_API_BASE.to_string(),
            api_key: String::new().into(),
        }
    }
}

impl async_openai::config::Config for OllamaConfig {
    fn headers(&self) -> HeaderMap {
        HeaderMap::new()
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
