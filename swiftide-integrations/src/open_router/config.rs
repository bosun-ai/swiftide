use derive_builder::Builder;
use reqwest::header::{HeaderMap, AUTHORIZATION};
use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;

const OPENROUTER_API_BASE: &str = "https://openrouter.ai/api/v1";

#[derive(Clone, Debug, Deserialize, Builder)]
#[serde(default)]
#[builder(setter(into, strip_option))]
pub struct OpenRouterConfig {
    #[builder(default = OPENROUTER_API_BASE.to_string())]
    api_base: String,
    api_key: SecretString,
    /// Sets the HTTP-Referer header (leaderbord)
    site_url: Option<String>,
    /// Sets the name (leaderbord)
    site_name: Option<String>,
}

impl OpenRouterConfig {
    pub fn builder() -> OpenRouterConfigBuilder {
        OpenRouterConfigBuilder::default()
    }
    pub fn with_api_base(&mut self, api_base: &str) -> &mut Self {
        self.api_base = api_base.to_string();

        self
    }

    pub fn with_api_key(&mut self, api_key: impl Into<SecretString>) -> &mut Self {
        self.api_key = api_key.into();

        self
    }
    pub fn with_site_url(&mut self, site_url: &str) -> &mut Self {
        self.site_url = Some(site_url.to_string());

        self
    }

    pub fn with_site_name(&mut self, site_name: &str) -> &mut Self {
        self.site_name = Some(site_name.to_string());

        self
    }
}

impl Default for OpenRouterConfig {
    fn default() -> Self {
        Self {
            api_base: OPENROUTER_API_BASE.to_string(),
            api_key: std::env::var("OPENROUTER_API_KEY")
                .unwrap_or_else(|_| String::new())
                .into(),
            site_url: None,
            site_name: None,
        }
    }
}

impl async_openai::config::Config for OpenRouterConfig {
    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();

        let api_key = self.api_key.expose_secret();
        assert!(!api_key.is_empty(), "API key for OpenRouter is required");

        headers.insert(
            AUTHORIZATION,
            format!("Bearer {}", self.api_key.expose_secret())
                .as_str()
                .parse()
                .unwrap(),
        );
        if let Ok(site_url) = self
            .site_url
            .as_deref()
            .unwrap_or("https://github.com/bosun-ai/swiftide")
            .parse()
        {
            headers.insert("HTTP-Referer", site_url);
        }

        if let Ok(site_name) = self.site_url.as_deref().unwrap_or("Swiftide").parse() {
            headers.insert("X-Title", site_name);
        }

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
