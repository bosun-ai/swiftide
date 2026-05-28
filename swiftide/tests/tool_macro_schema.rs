use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use swiftide::{
    chat_completion::{Tool, ToolOutput, errors::ToolError},
    traits::AgentContext,
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum MacroPriority {
    Low,
    Normal,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct MacroTicketRequest {
    /// Support ticket title.
    title: String,
    /// Priority to assign to the ticket.
    priority: MacroPriority,
}

#[swiftide::tool(
    description = "Triage a support ticket with a fixed priority",
    param(name = "request", description = "Support ticket triage request")
)]
async fn triage_ticket(
    _context: &dyn AgentContext,
    request: MacroTicketRequest,
) -> Result<ToolOutput, ToolError> {
    Ok(ToolOutput::text(format!(
        "triaged `{}` as {:?}",
        request.title, request.priority
    )))
}

#[test]
fn tool_macro_schema_is_provider_ready_for_nested_enum_arguments() {
    let schema = triage_ticket()
        .tool_spec()
        .canonical_parameters_schema_json()
        .unwrap();

    assert_eq!(schema.get("$defs"), None);
    assert_eq!(
        schema.pointer("/properties/request/properties/priority"),
        Some(&json!({
            "description": "Priority to assign to the ticket.",
            "enum": ["low", "normal", "high"],
            "type": "string"
        }))
    );
}
