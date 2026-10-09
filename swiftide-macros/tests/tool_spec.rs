use swiftide::chat_completion::{
    ChatCompletionRequest, ChatMessage, Tool as _, ToolOutput, errors::ToolError,
};
use swiftide::traits::AgentContext;
use swiftide_macros::Tool;

#[swiftide_macros::tool(
    description = "Searches indexed source code",
    param(name = "query", description = "Search text used to find matching code")
)]
async fn described_attribute_tool(
    _agent_context: &dyn AgentContext,
    query: &str,
) -> Result<ToolOutput, ToolError> {
    Ok(format!("Searching for {query}").into())
}

/// Searches indexed notes.
///
/// # Arguments
/// - `_agent_context`: Context for the current agent.
/// - `query`: Text to search for. Matching is case-insensitive.
/// - `limit`: Maximum number of notes to return.
#[swiftide_macros::tool]
async fn documented_attribute_tool(
    _agent_context: &dyn AgentContext,
    query: &str,
    limit: usize,
) -> Result<ToolOutput, ToolError> {
    Ok(format!("Searching for {query}, limited to {limit}").into())
}

/// Documentation used only when the matching attribute value is omitted.
///
/// # Arguments
/// - `query`: Documentation fallback.
#[swiftide_macros::tool(
    description = "Explicit tool description",
    param(name = "query", description = "Explicit parameter description")
)]
async fn explicit_attribute_tool(
    _agent_context: &dyn AgentContext,
    query: &str,
) -> Result<ToolOutput, ToolError> {
    Ok(format!("Searching for {query}").into())
}

#[derive(Clone, Tool)]
#[tool(
    description = "Searches indexed source code",
    param(name = "query", description = "Search text used to find matching code")
)]
struct DescribedDeriveTool;

impl DescribedDeriveTool {
    async fn described_derive_tool(
        &self,
        _agent_context: &dyn AgentContext,
        query: &str,
    ) -> Result<ToolOutput, ToolError> {
        tokio::task::yield_now().await;
        Ok(format!("Searching for {query}").into())
    }
}

#[test]
fn attribute_macro_exposes_argument_description_in_request_tool_spec() {
    let request = ChatCompletionRequest::builder()
        .messages(vec![ChatMessage::User("search the repo".into())])
        .tool(described_attribute_tool())
        .build()
        .unwrap();

    let spec = request
        .tools_spec()
        .iter()
        .find(|spec| spec.name == "described_attribute_tool")
        .expect("attribute macro tool spec should be attached to the request");

    assert_eq!(
        argument_description(spec, "query").as_deref(),
        Some("Search text used to find matching code")
    );
}

#[test]
fn derive_macro_exposes_argument_description_in_tool_spec() {
    let spec = DescribedDeriveTool.tool_spec();

    assert_eq!(
        argument_description(&spec, "query").as_deref(),
        Some("Search text used to find matching code")
    );
}

#[test]
fn attribute_macro_reads_summary_and_arguments_from_doc_comments() {
    let spec = documented_attribute_tool().tool_spec();

    assert_eq!(spec.description, "Searches indexed notes.");
    assert_eq!(
        argument_description(&spec, "query").as_deref(),
        Some("Text to search for. Matching is case-insensitive.")
    );
    assert_eq!(
        argument_description(&spec, "limit").as_deref(),
        Some("Maximum number of notes to return.")
    );
    assert_eq!(argument_description(&spec, "_agent_context"), None);
}

#[test]
fn attribute_values_override_doc_comment_fallbacks() {
    let spec = explicit_attribute_tool().tool_spec();

    assert_eq!(spec.description, "Explicit tool description");
    assert_eq!(
        argument_description(&spec, "query").as_deref(),
        Some("Explicit parameter description")
    );
}

fn argument_description(
    spec: &swiftide::chat_completion::ToolSpec,
    argument_name: &str,
) -> Option<String> {
    spec.canonical_parameters_schema_json()
        .ok()?
        .get("properties")?
        .get(argument_name)?
        .get("description")?
        .as_str()
        .map(ToOwned::to_owned)
}
