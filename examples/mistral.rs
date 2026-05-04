//! Demonstrates prompting Mistral AI's OpenAI-compatible chat completion endpoint.
//!
//! Set the `MISTRAL_API_KEY` environment variable before running.

use swiftide::{integrations::mistral::Mistral, traits::SimplePrompt};

const MODELS: &[&str] = &["devstral-latest", "mistral-large-latest"];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    for model in MODELS {
        let client = Mistral::builder()
            .default_prompt_model(*model)
            .to_owned()
            .build()?;

        let response = client
            .prompt(
                "Reply with one short sentence that names your model family and says Swiftide works."
                    .into(),
            )
            .await?;

        println!("{model}: {response}");
    }

    Ok(())
}
