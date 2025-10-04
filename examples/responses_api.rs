use anyhow::Result;
use schemars::JsonSchema;
use serde::Deserialize;
use swiftide::{
    integrations::openai::{OpenAI, Options},
    traits::{SimplePrompt, StructuredPrompt},
};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WeatherSummary {
    description: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let openai = OpenAI::builder()
        .default_prompt_model("gpt-4.1-mini")
        .default_options(Options::builder().temperature(0.2))
        .use_responses_api(true)
        .build()?;

    let greeting = openai
        .prompt("Say hello in one short sentence".into())
        .await?;
    println!("Prompt result: {greeting}");

    let structured: WeatherSummary = openai
        .structured_prompt("Summarise today's weather in Amsterdam as JSON".into())
        .await?;
    println!("Structured result: {structured:?}");

    Ok(())
}
