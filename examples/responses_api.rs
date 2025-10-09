use anyhow::Result;
use futures_util::StreamExt as _;
use schemars::JsonSchema;
use serde::Deserialize;
use std::io::Write as _;
use swiftide::{
    chat_completion::{ChatCompletionRequest, ChatMessage},
    integrations::openai::{OpenAI, Options},
    traits::{ChatCompletion, SimplePrompt, StructuredPrompt},
};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
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

    // let greeting = openai
    //     .prompt("Say hello in one short sentence".into())
    //     .await?;
    // println!("Prompt result: {greeting}");
    //
    // let structured: WeatherSummary = openai
    //     .structured_prompt("Summarise today's weather in Amsterdam as JSON".into())
    //     .await?;
    // println!("Structured result: {structured:?}");
    //
    let chat_request = ChatCompletionRequest::builder()
        .messages(vec![
            ChatMessage::new_system("You are a concise assistant."),
            ChatMessage::new_user("Share one fun fact about Amsterdam."),
        ])
        .build()?;
    //
    // let completion = openai.complete(&chat_request).await?;
    // println!(
    //     "Complete result: {}",
    //     completion.message().unwrap_or("<no message>")
    // );

    let mut stream = openai.complete_stream(&chat_request).await;
    print!("Streaming result: ");
    let mut streamed_message = String::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if let Some(delta) = chunk
            .delta
            .as_ref()
            .and_then(|delta| delta.message_chunk.as_deref())
        {
            print!("{delta}");
            std::io::stdout().flush().ok();
        }

        if let Some(message) = chunk.message() {
            streamed_message = message.to_string();
        }
    }
    println!();
    if streamed_message.is_empty() {
        println!("Full streamed result: <no message>");
    } else {
        println!("Full streamed result: {streamed_message}");
    }

    Ok(())
}
