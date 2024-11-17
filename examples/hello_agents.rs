use anyhow::Result;
use indoc::indoc;
use swiftide::agents;

#[tokio::main]
async fn main() -> Result<()> {
    println!("Hello, agents!");

    let openai = swiftide::integrations::openai::OpenAI::builder()
        .default_embed_model("text-embeddings-3-small")
        .default_prompt_model("gpt-4o-mini")
        .build()?;

    agents::Agent::builder()
        .llm(&openai)
        .build()?
        .run("What is the meaning of life?")
        .await?;

    Ok(())
}
