//! This is an example of using the langfuse integration with Swiftide.
//!
//! Langfuse is a platform for tracking and monitoring LLM usage and performance.
//!
//! When the feature `langfuse` is enabled, Swiftide can report tracing information,
//! usage, inputs, and outputs to langfuse.
//!
//! For this to work, you need to set the LANGFUSE_PUBLIC_KEY and LANGFUSE_SECRET_KEY
//! to the appropriate values. You can also set the LANGFUSE_URL environment variable
//! to overwrite the default URL (http://localhost:3000).
//!
//! You can find more information about langfuse at https://langfuse.com/. On their github they
//! also have a handy docker compose setup.
//!
//! More advanced usage is possible by using the `LangfuseLayer` directly.
use anyhow::Result;
use swiftide::traits::SimplePrompt;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{
    EnvFilter, Layer as _, layer::SubscriberExt as _, util::SubscriberInitExt as _,
};

#[tokio::main]
async fn main() -> Result<()> {
    println!("Hello, langfuse!");

    let fmt_layer = tracing_subscriber::fmt::layer()
        .compact()
        .with_target(false)
        .boxed();

    let langfuse_layer = swiftide::langfuse::LangfuseLayer::default()
        .with_filter(LevelFilter::DEBUG)
        .boxed();

    let registry = tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(vec![fmt_layer, langfuse_layer]);

    registry.init();

    prompt_openai().await?;

    Ok(())
}

#[tracing::instrument]
async fn prompt_openai() -> Result<()> {
    let openai = swiftide::integrations::openai::OpenAI::builder()
        .default_prompt_model("gpt-5")
        .build()
        .unwrap();

    let paris = openai
        .prompt("What is the capital of France?".into())
        .await?;

    println!("The capital of France is {paris}");

    Ok(())
}
