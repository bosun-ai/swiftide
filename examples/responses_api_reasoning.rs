//! Simple agent example that enables reasoning summaries via the Responses API.

use anyhow::Result;
use swiftide::agents::Agent;
use swiftide::chat_completion::{ChatMessage, ReasoningItem};
use swiftide::integrations::openai::{OpenAI, Options, ReasoningEffort};
use tracing_subscriber::EnvFilter;

fn reasoning_summary(reasoning: Option<&[ReasoningItem]>) -> Option<String> {
    let summary = reasoning
        .unwrap_or(&[])
        .iter()
        .flat_map(|item| item.summary.iter())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");

    if summary.is_empty() {
        None
    } else {
        Some(summary)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Reasoning models require the Responses API. Enabling reasoning effort also asks for a
    // summary and encrypted reasoning content (enabled by default). If your OpenAI org is not
    // verified for reasoning access, summaries may be absent. Disable with
    // `reasoning_features(false)` if desired.
    let openai = OpenAI::builder()
        .default_prompt_model("o3-mini")
        .default_options(Options::builder().reasoning_effort(ReasoningEffort::Low))
        .use_responses_api(true)
        .build()?;

    let mut agent = Agent::builder()
        .llm(&openai)
        .on_new_message(|_, message| {
            if let ChatMessage::Assistant(assistant) = message {
                if let Some(content) = assistant.content.as_deref() {
                    println!("Assistant: {content}");
                }
            }
            Box::pin(async move { Ok(()) })
        })
        .after_completion(|_, response| {
            if let Some(summary) = reasoning_summary(response.reasoning.as_deref()) {
                println!("Reasoning summary:\n{summary}");
            }

            let has_encrypted = response
                .reasoning
                .as_ref()
                .is_some_and(|items| items.iter().any(|item| item.encrypted_content.is_some()));
            println!("Encrypted reasoning content present: {has_encrypted}");
            Box::pin(async move { Ok(()) })
        })
        .build()?;

    agent
        .query("Explain why the sky is blue in one short paragraph.")
        .await?;

    Ok(())
}
