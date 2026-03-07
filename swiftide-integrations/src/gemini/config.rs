use reqwest::header::{AUTHORIZATION, HeaderMap};
use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;

const GEMINI_API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/openai";

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct GeminiConfig {
    api_base: String,
    api_key: SecretString,
}

impl Default for GeminiConfig {
    fn default() -> Self {
        Self {
            api_base: GEMINI_API_BASE.to_string(),
            api_key: std::env::var("GEMINI_API_KEY")
                .unwrap_or_else(|_| String::new())
                .into(),
        }
    }
}

impl async_openai::config::Config for GeminiConfig {
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
