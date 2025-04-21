//! This example demonstrates how to stream responses from an agent
use anyhow::Result;
use swiftide::agents;

#[tokio::main]
async fn main() -> Result<()> {
    let openai = swiftide::integrations::openai::OpenAI::builder()
        .default_embed_model("text-embeddings-3-small")
        .default_prompt_model("gpt-4o-mini")
        .build()?;

    agents::Agent::builder()
        .llm(&openai)
        .on_stream(|_agent, response| {
            // We print the message chunk if it exists. Streamed responses also include
            // the full response (without tool calls) in `message` and an `id` to map them to
            // previous chunks for convenience.
            //
            // The agent uses the full assembled response at the end of the stream.
            if let Some(delta) = &response.delta {
                print!(
                    "{}",
                    delta
                        .message_chunk
                        .as_deref()
                        .map(str::to_string)
                        .unwrap_or_default()
                );
            };

            Box::pin(async move { Ok(()) })
        })
        // Every message added by the agent will be printed to stdout
        .on_new_message(move |_, msg| {
            let msg = msg.to_string();
            Box::pin(async move {
                println!("\n---\nFinal message:\n {msg}");
                Ok(())
            })
        })
        .limit(5)
        .build()?
        .query("Why is the rust programming language so good?")
        .await?;

    Ok(())
}
