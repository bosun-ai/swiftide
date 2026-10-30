use async_trait::async_trait;
use std::sync::Arc;
use swiftide_agents::Agent;
use swiftide_agents_contrib::hooks::{ContextCompression, ToolSummarizer};
use swiftide_core::{
    ChatCompletion,
    chat_completion::{ChatCompletionRequest, ChatCompletionResponse, errors::LanguageModelError},
};

/// Replace this offline model with the application's preferred provider.
#[derive(Clone, Debug)]
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
    let output = ToolSummarizer::builder()
        .llm(Arc::new(ExampleModel) as Arc<dyn swiftide_core::SimplePrompt>)
        .tools_to_summarize(Vec::<&'static str>::new())
        .git_start_sha("HEAD")
        .build()?;
    let conversation = ContextCompression::builder()
        .llm(Arc::new(ExampleModel) as Arc<dyn ChatCompletion>)
        .num_completions_for_summary(5usize)
        .git_start_sha("HEAD")
        .git_diff_dir(".")
        .context_limit(32_000usize)
        .build()?;
    let mut builder = Agent::builder();
    conversation.register_hooks(&mut builder);
    let mut agent = builder
        .llm(&ExampleModel)
        .after_tool(output.summarize_hook())
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

#[async_trait]
impl swiftide_core::SimplePrompt for ExampleModel {
    async fn prompt(
        &self,
        _prompt: swiftide_core::prompt::Prompt,
    ) -> Result<String, LanguageModelError> {
        Ok("Example tool summary".into())
    }
}
