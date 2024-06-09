use dotenvy::dotenv;
use std::env;
use std::sync::OnceLock;

static CONFIG: OnceLock<Config> = OnceLock::new();

pub struct Config {
    pub port: String,
    pub openai_api_key: String,
    pub otel_enabled: bool,
    pub openai_endpoint: Option<String>,
    pub github_app_id: Option<u64>,
    pub qdrant_url: Option<String>,
    pub qdrant_api_key: Option<String>,
    pub redis_url: Option<String>,
}

impl Config {
    pub fn from_env() -> &'static Config {
        CONFIG.get_or_init(|| {
            tracing::info!("Loading config from environment");
            dotenv().ok();

            let port = env::var("PORT").expect("PORT env var not set");
            let openai_api_key =
                env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY env var not set");
            let otel_enabled = env::var("OTEL_ENABLED")
                .expect("OTEL_ENABLED env var not set")
                .parse::<bool>()
                .expect("OTEL_ENABLED env var must be a boolean");
            let openai_endpoint = env::var("OPENAI_ENDPOINT").ok();

            let github_app_id: Option<u64> = env::var("GITHUB_APP_ID")
                .map(|s| s.parse::<u64>().expect("GITHUB_APP_ID must be a number"))
                .ok();

            let qdrant_url = env::var("QDRANT_URL").ok();
            let qdrant_api_key = env::var("QDRANT_API_KEY").ok();
            let redis_url = env::var("REDIS_URL").ok();

            Self {
                port,
                openai_api_key,
                otel_enabled,
                openai_endpoint,
                github_app_id,
                qdrant_url,
                qdrant_api_key,
                redis_url,
            }
        })
    }

    pub fn otel_enabled(&self) -> bool {
        self.otel_enabled
    }
}
