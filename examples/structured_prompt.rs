use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swiftide::{integrations, traits::DynStructuredPrompt, traits::StructuredPrompt as _};

#[tokio::main]
async fn main() -> Result<()> {
    // ANCHOR: setup
    let client = integrations::openai::OpenAI::builder()
        .default_prompt_model("gpt-5-mini")
        .build()?;
    // ANCHOR_END: setup

    // ANCHOR: schema
    // Note that deny unknown fields is required. If you get an error on 'additionalProperties' to
    // be required, and false, this is what is missing.
    #[derive(Deserialize, JsonSchema, Serialize, Debug)]
    #[serde(deny_unknown_fields)]
    struct MyResponse {
        questions: Vec<String>,
    }
    // ANCHOR_END: schema

    // ANCHOR: prompt
    let response = client
        .structured_prompt::<MyResponse>(
            "List three interesting questions about the Rust programming language.".into(),
        )
        .await?;

    println!("Response: {:?}", response.questions);
    // ANCHOR_END: prompt

    // ANCHOR: dynamic
    // Because we use generics, structured_prompt is not dyn safe. However, there is an
    // alternative:

    let client: Box<dyn DynStructuredPrompt> = Box::new(client);

    let response: serde_json::Value = client
        .structured_prompt_dyn(
            "List three interesting questions about the Rust programming language.".into(),
            schemars::schema_for!(MyResponse),
        )
        .await?;

    let parsed: MyResponse = serde_json::from_value(response)?;

    println!("Response: {:?}", parsed);
    // ANCHOR_END: dynamic

    Ok(())
}
