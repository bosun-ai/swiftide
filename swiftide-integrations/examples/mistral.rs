//! Direct Mistral Chat Completions or Responses, without Bifrost or another gateway.
//!
//! Set `MISTRAL_API_KEY`, then run:
//! `cargo run -p swiftide-integrations --features mistral --example mistral`
//! Add `-- --stream` for streaming or `-- --responses --stream` for the Responses preview.
//! Defaults: `MISTRAL_MODEL=mistral-large-4` and
//! `MISTRAL_REASONING_EFFORT=high`. Large 4 also supports `low` and `medium`.
//! Running this example makes two real provider requests and can incur charges.
//!
//! The first turn calls an in-memory echo tool; the second replays reasoning, assistant/tool
//! messages, and returns the answer. A stable `MISTRAL_PROMPT_CACHE_KEY` (default:
//! `swiftide-mistral-example`) encourages reuse of the shared prefix; cache hits are not
//! guaranteed. Usage includes cached input and reasoning tokens. Reasoning stays separate from
//! answer text.
//!
//! Provider contract: <https://docs.mistral.ai/resources/migration-guides> and
//! <https://docs.mistral.ai/studio/conversations/reasoning>.

use anyhow::{Context as _, Result, ensure};
use futures_util::StreamExt as _;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use swiftide_core::chat_completion::{
    ChatCompletion, ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ToolSpec,
};
use swiftide_integrations::mistral::{Mistral, OpenAIConfig, Options};

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct EchoArgs {
    text: String,
}

async fn complete(
    client: &Mistral,
    request: &ChatCompletionRequest<'_>,
) -> Result<ChatCompletionResponse> {
    let response = if std::env::args().any(|argument| argument == "--stream") {
        let mut stream = client.complete_stream(request).await;
        let mut final_response = None;
        while let Some(response) = stream.next().await {
            final_response = Some(response?);
        }
        final_response.context("Empty stream")?
    } else {
        client.complete(request).await?
    };
    let reasoning_characters: usize = response
        .reasoning
        .iter()
        .flatten()
        .flat_map(|item| item.content.iter().flatten())
        .map(|text| text.chars().count())
        .sum();
    println!("Reasoning received separately: {reasoning_characters} characters");
    Ok(response)
}

#[tokio::main]
async fn main() -> Result<()> {
    let api_key = std::env::var("MISTRAL_API_KEY").context("Set MISTRAL_API_KEY")?;
    let model = std::env::var("MISTRAL_MODEL").unwrap_or_else(|_| "mistral-large-4".into());
    let config = OpenAIConfig::new()
        .with_api_base("https://api.mistral.ai/v1")
        .with_api_key(api_key);

    let mut options = Options::builder();
    // Model names have no gateway prefix for direct Mistral requests.
    options.max_tokens(4096);
    options.extra_body(Map::from_iter([(
        "prompt_cache_key".into(),
        json!(
            std::env::var("MISTRAL_PROMPT_CACHE_KEY")
                .unwrap_or_else(|_| "swiftide-mistral-example".into())
        ),
    )]));
    let effort = std::env::var("MISTRAL_REASONING_EFFORT").unwrap_or_else(|_| "high".into());
    options.reasoning_effort(serde_json::from_value::<
        swiftide_integrations::mistral::ReasoningEffort,
    >(Value::String(effort))?);

    let (usage_tx, mut usage_rx) = tokio::sync::mpsc::unbounded_channel();
    let client = Mistral::builder()
        .client(async_openai::Client::with_config(config))
        .use_responses_api(std::env::args().any(|argument| argument == "--responses"))
        .default_prompt_model(model)
        .default_options(options)
        .on_usage(move |usage| {
            usage_tx.send(usage.clone())?;
            Ok(())
        })
        .build()?;
    let mut request = ChatCompletionRequest::builder()
        .messages(vec![
            ChatMessage::new_system("You are testing a local echo tool. Use the echo tool exactly once when asked to echo a word. After receiving its result, reply with only the echoed word and do not call any more tools. The tool has no side effects and returns exactly the supplied text. Keep the tool arguments identical to the word requested by the user. Never invent a tool result; wait for the actual tool response before giving your final answer. Keep the final answer short, without explanations or formatting."),
            ChatMessage::new_user("Echo bonjour using the tool."),
        ])
        .tool_specs([ToolSpec::builder().name("echo").description("Echo the supplied text")
            .parameters_schema(schemars::schema_for!(EchoArgs)).build()?])
        .build()?;

    let result: Result<()> = async {
        let response = complete(&client, &request).await?;
        let calls = response.tool_calls.context("Expected an echo tool call")?;
        ensure!(
            calls.len() == 1 && calls[0].name() == "echo",
            "Expected exactly one echo tool call"
        );
        let arguments: EchoArgs =
            serde_json::from_str(calls[0].args().context("Missing tool arguments")?)?;
        // Swiftide agents retain this history automatically. Manual callers must also keep the
        // reasoning before the assistant message, including on tool-use turns.
        for item in response.reasoning.into_iter().flatten() {
            request
                .messages
                .to_mut()
                .push(ChatMessage::new_reasoning(item));
        }
        request.messages.to_mut().push(ChatMessage::new_assistant(
            response.message,
            Some(calls.clone()),
        ));
        request.messages.to_mut().push(ChatMessage::new_tool_output(
            calls[0].clone(),
            arguments.text,
        ));
        let response = complete(&client, &request).await?;
        ensure!(
            response.tool_calls.as_ref().is_none_or(Vec::is_empty),
            "Expected final answer, not another tool call"
        );
        println!("{}", response.message().context("Expected final answer")?);
        Ok(())
    }
    .await;

    // Drain the spawned accounting callbacks on both success and failure before exiting.
    // A response without reported usage is unknown spend, not zero spend.
    drop(client);
    while let Some(usage) = usage_rx.recv().await {
        eprintln!("Provider-reported usage: {usage:?}");
    }
    result
}
