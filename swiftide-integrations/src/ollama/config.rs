use derive_builder::Builder;
use reqwest::header::HeaderMap;
use secrecy::SecretString;
use serde::Deserialize;

const OLLAMA_API_BASE: &str = "http://localhost:11434/v1";

#[derive(Clone, Debug, Deserialize, Builder)]
#[serde(default)]
pub struct OllamaConfig {
    api_base: String,
    api_key: SecretString,
}

impl OllamaConfig {
    pub fn builder() -> OllamaConfigBuilder {
        OllamaConfigBuilder::default()
    }

    pub fn with_api_base(&mut self, api_base: &str) -> &mut Self {
        self.api_base = api_base.to_string();

        self
    }
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

    fn api_key(&self) -> &SecretString {
        &self.api_key
    }

    fn query(&self) -> Vec<(&str, &str)> {
        vec![]
    }
}
