use async_trait::async_trait;
use std::sync::Arc;
use swiftide_agents::Agent;
use swiftide_agents_contrib::hooks::{ContextCompression, ToolSummarizer};
use swiftide_core::{
    ChatCompletion,
    chat_completion::{ChatCompletionRequest, ChatCompletionResponse, errors::LanguageModelError},
};

/// Replace this offline model with the application's preferred provider.
#[derive(Clone)]
struct ExampleModel;

#[async_trait]
impl ChatCompletion for ExampleModel {
    async fn complete(
        &self,
        _request: &ChatCompletionRequest<'_>,
    ) -> Result<ChatCompletionResponse, LanguageModelError> {
        Ok(ChatCompletionResponse {
            message: Some("Example response; the original goal remains recoverable.".into()),
            ..Default::default()
        })
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let output = ToolSummarizer::builder().max_output_chars(40_000).build()?;
    let conversation = ContextCompression::builder()
        .llm(Arc::new(ExampleModel))
        .summarize_every_turns(1)
        .context_limit(32_000)
        .build()?;
    let mut agent = Agent::builder()
        .llm(&ExampleModel)
        .after_tool(output.summarize_hook())
        .after_each(conversation.summarize_hook())
        .build()?;
    agent
        .query_once("Demonstrate conversation compression without network access.")
        .await?;
    println!(
        "History contains {} messages",
        agent.context().history().await?.len()
    );
    Ok(())
}
