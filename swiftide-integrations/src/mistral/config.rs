use reqwest::header::{AUTHORIZATION, HeaderMap};
use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;

const MISTRAL_API_BASE: &str = "https://api.mistral.ai/v1";

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct MistralConfig {
    api_base: String,
    api_key: SecretString,
}

impl Default for MistralConfig {
    fn default() -> Self {
        Self {
            api_base: MISTRAL_API_BASE.to_string(),
            api_key: std::env::var("MISTRAL_API_KEY")
                .unwrap_or_else(|_| String::new())
                .into(),
        }
    }
}

impl async_openai::config::Config for MistralConfig {
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

    fn api_key(&self) -> &SecretString {
        &self.api_key
    }

    fn query(&self) -> Vec<(&str, &str)> {
        vec![]
    }
}
