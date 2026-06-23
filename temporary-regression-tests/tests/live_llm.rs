#![cfg(any(
    feature = "live-openai",
    feature = "live-anthropic",
    feature = "live-bedrock"
))]

use anyhow::{Context as _, Result};
use schemars::schema_for;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use swiftide::chat_completion::{ChatCompletionRequest, ChatCompletionResponse, ChatMessage};
use swiftide::traits::ChatCompletion;
use swiftide_core::chat_completion::ToolSpec;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct NestedCommentArgs {
    request: NestedCommentRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct NestedCommentRequest {
    body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    page_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    block_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    discussion_id: Option<String>,
}

fn env_required(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("{name} must be set for this live regression test"))
}

fn install_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

fn comment_tool_spec() -> Result<ToolSpec> {
    Ok(ToolSpec::builder()
        .name("create_comment")
        .description("Create a comment. Only include fields the user explicitly provided.")
        .parameters_schema(schema_for!(NestedCommentArgs))
        .build()?)
}

fn comment_request() -> Result<ChatCompletionRequest<'static>> {
    Ok(ChatCompletionRequest::builder()
        .messages(vec![
            ChatMessage::new_system(
                "You must call the create_comment tool exactly once. \
                 Put the user supplied sentence in request.body. \
                 Do not include request.text, request.page_id, request.block_id, \
                 or request.discussion_id unless the user explicitly supplies them.",
            ),
            ChatMessage::new_user(
                "Create one comment with body: Regression harness body. No other fields are supplied.",
            ),
        ])
        .tool_specs([comment_tool_spec()?])
        .build()?)
}

fn assert_tool_args_deserialize(response: ChatCompletionResponse) -> Result<()> {
    let tool_calls = response
        .tool_calls()
        .context("provider did not return any tool calls")?;
    let tool_call = tool_calls
        .first()
        .context("provider returned an empty tool call list")?;

    assert_eq!(tool_call.name(), "create_comment");

    let args: Value = serde_json::from_str(
        tool_call
            .args()
            .context("create_comment tool call had no arguments")?,
    )?;
    let typed: NestedCommentArgs = serde_json::from_value(args.clone())?;
    let request = args
        .get("request")
        .and_then(Value::as_object)
        .context("tool arguments should contain an object at `request`")?;

    assert_eq!(
        request.get("body").and_then(Value::as_str),
        Some("Regression harness body")
    );

    println!(
        "optional field representation: text={:?}, page_id={:?}, block_id={:?}, discussion_id={:?}",
        request.get("text"),
        request.get("page_id"),
        request.get("block_id"),
        request.get("discussion_id")
    );
    assert_eq!(typed.request.body, "Regression harness body");

    Ok(())
}

async fn run_optional_nested_fields_regression(
    provider: &impl ChatCompletion,
) -> Result<ChatCompletionResponse> {
    provider
        .complete(&comment_request()?)
        .await
        .map_err(Into::into)
}

#[cfg(feature = "live-openai")]
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires OPENAI_API_KEY and SWIFTIDE_REGRESSION_OPENAI_MODEL"]
async fn openai_omits_unsupplied_nested_optional_tool_fields() -> Result<()> {
    install_crypto_provider();

    let model = env_required("SWIFTIDE_REGRESSION_OPENAI_MODEL")?;
    let provider = swiftide::integrations::openai::OpenAI::builder()
        .default_prompt_model(model)
        .default_embed_model("text-embedding-3-small")
        .build()?;

    let response = run_optional_nested_fields_regression(&provider).await?;
    assert_tool_args_deserialize(response)
}

#[cfg(feature = "live-anthropic")]
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires ANTHROPIC_API_KEY and SWIFTIDE_REGRESSION_ANTHROPIC_MODEL"]
async fn anthropic_omits_unsupplied_nested_optional_tool_fields() -> Result<()> {
    install_crypto_provider();

    let model = env_required("SWIFTIDE_REGRESSION_ANTHROPIC_MODEL")?;
    let provider = swiftide::integrations::anthropic::Anthropic::builder()
        .default_prompt_model(model)
        .build()?;

    let response = run_optional_nested_fields_regression(&provider).await?;
    assert_tool_args_deserialize(response)
}

#[cfg(feature = "live-bedrock")]
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires AWS credentials, AWS_REGION, and SWIFTIDE_REGRESSION_BEDROCK_MODEL"]
async fn bedrock_omits_unsupplied_nested_optional_tool_fields() -> Result<()> {
    install_crypto_provider();

    let model = env_required("SWIFTIDE_REGRESSION_BEDROCK_MODEL")?;
    let provider = swiftide::integrations::aws_bedrock_v2::AwsBedrock::builder()
        .default_prompt_model(model)
        .build()?;

    let response = run_optional_nested_fields_regression(&provider).await?;
    assert_tool_args_deserialize(response)
}
