//! This example demonstrates how to stream responses from an agent
//!
//! By default, for convenience the accumulated response is streamed. You can opt-out of this
//! behaviour and only receive the delta as well (only with OpenAI-like providers).
use anyhow::Result;
use swiftide::agents;

#[tokio::main]
async fn main() -> Result<()> {
    let openai = swiftide::integrations::openai::OpenAI::builder()
        .default_embed_model("text-embeddings-3-small")
        .default_prompt_model("gpt-4o-mini")
        // Only streams the delta, leave this out to stream the full response
        .stream_full(false)
        .build()?;

    // let anthropic = swiftide::integrations::anthropic::Anthropic::builder()
    //     .default_prompt_model("claude-3-7-sonnet-latest")
    //     .build()?;

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
            // If `stream_full` is disabled, response.message() will be the accumulated response
            // response.message()

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
