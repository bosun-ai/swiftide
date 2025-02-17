use reqwest::header::{HeaderMap, AUTHORIZATION};
use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;

const GROQ_API_BASE: &str = "https://api.groq.com/openai/v1";

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct GroqConfig {
    api_base: String,
    api_key: SecretString,
}

impl Default for GroqConfig {
    fn default() -> Self {
        Self {
            api_base: GROQ_API_BASE.to_string(),
            api_key: std::env::var("GROQ_API_KEY")
                .unwrap_or_else(|_| String::new())
                .into(),
        }
    }
}

impl async_openai::config::Config for GroqConfig {
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
