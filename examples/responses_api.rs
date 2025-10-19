use anyhow::{Context, Result};
use futures_util::StreamExt as _;
use schemars::JsonSchema;
use serde::Deserialize;
use std::collections::HashSet;
use std::io::Write as _;
use swiftide::{
    chat_completion::{ChatCompletionRequest, ChatMessage, ToolOutput, ToolSpec},
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

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EchoArgs {
    message: String,
}

fn echo_tool_spec() -> ToolSpec {
    ToolSpec::builder()
        .name("echo_tool")
        .description("Echos the provided message back to the caller")
        .parameters_schema(schemars::schema_for!(EchoArgs))
        .build()
        .expect("echo tool spec must be valid")
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

    let greeting = openai
        .prompt("Say hello in one short sentence".into())
        .await?;
    println!("Prompt result: {greeting}");

    let structured: WeatherSummary = openai
        .structured_prompt("Summarise today's weather in Amsterdam as JSON".into())
        .await?;
    println!("Structured result: {structured:?}");

    let chat_request = ChatCompletionRequest::builder()
        .messages(vec![
            ChatMessage::new_system("You are a concise assistant."),
            ChatMessage::new_user("Share one fun fact about Amsterdam."),
        ])
        .build()?;

    let completion = openai.complete(&chat_request).await?;
    println!(
        "Complete result: {}",
        completion.message().unwrap_or("<no message>")
    );

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

    let echo_spec = echo_tool_spec();
    let tool_specs: HashSet<_> = std::iter::once(echo_spec.clone()).collect();

    let tool_request = ChatCompletionRequest::builder()
        .messages(vec![
            ChatMessage::new_system(
                "You are a precise assistant. Use available tools before replying directly.",
            ),
            ChatMessage::new_user(
                "Call the echo tool with the phrase \"Hello Responses API\" and then summarise the result.",
            ),
        ])
        .tools_spec(tool_specs.clone())
        .build()?;

    let tool_completion = openai.complete(&tool_request).await?;

    if let Some(tool_call) = tool_completion
        .tool_calls()
        .and_then(|calls| calls.first())
        .cloned()
    {
        println!(
            "Assistant requested tool `{}` with arguments {}",
            tool_call.name(),
            tool_call.args().unwrap_or("<missing arguments>")
        );

        let args_json = tool_call
            .args()
            .context("echo tool call missing arguments")?;
        let args: EchoArgs = serde_json::from_str(args_json)?;
        let tool_output = format!("Echo: {}", args.message);

        let mut follow_up_messages = tool_request.messages().to_vec();
        follow_up_messages.push(ChatMessage::new_assistant(
            None::<String>,
            Some(vec![tool_call.clone()]),
        ));
        follow_up_messages.push(ChatMessage::new_tool_output(
            tool_call.clone(),
            ToolOutput::text(tool_output),
        ));

        let follow_up_request = ChatCompletionRequest::builder()
            .messages(follow_up_messages)
            .tools_spec(tool_specs)
            .build()?;

        let final_completion = openai.complete(&follow_up_request).await?;
        println!(
            "Final response after tool call: {}",
            final_completion.message().unwrap_or("<no message>")
        );
    } else {
        println!(
            "Assistant responded without tool calls: {}",
            tool_completion.message().unwrap_or("<no message>")
        );
    }

    Ok(())
}
