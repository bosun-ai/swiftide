#![cfg(feature = "planning")]
use serde_json::json;
use swiftide_agents::DefaultContext;
use swiftide_agents_contrib::tools::UpdatePlan;
use swiftide_core::chat_completion::{Tool, ToolCall, ToolOutput};
async fn invoke(
    tool: &UpdatePlan,
    args: serde_json::Value,
) -> Result<ToolOutput, swiftide_core::chat_completion::errors::ToolError> {
    tool.invoke(
        &DefaultContext::default(),
        &ToolCall::builder()
            .id("plan")
            .name("update_plan")
            .args(args.to_string())
            .build()
            .unwrap(),
    )
    .await
}
#[tokio::test]
async fn plans_keep_source_rendering_trimming_clone_state_and_required_explanation() {
    let tool = UpdatePlan::default();
    let output = invoke(
        &tool,
        json!({"items":[{"step":"  Work  ","status":"in_progress"}],"explanation":"  progress  "}),
    )
    .await
    .unwrap();
    assert_eq!(
        output.as_text(),
        Some("# Current Plan\n1. [in_progress] Work\n\nprogress")
    );
    assert_eq!(
        invoke(&tool.clone(), json!({"items":[],"explanation":""}))
            .await
            .unwrap()
            .as_text(),
        Some("# Current Plan\n1. [in_progress] Work")
    );
    assert!(invoke(&tool, json!({"items":[]})).await.is_err());
    // The original macro accepts extra top-level keys.
    assert!(
        invoke(&tool, json!({"items":[],"explanation":"","extra":true}))
            .await
            .is_ok()
    );
}
#[tokio::test]
async fn source_validation_keeps_the_old_plan_and_accepts_multiline_steps() {
    let tool = UpdatePlan::default();
    invoke(
        &tool,
        json!({"items":[{"step":"Keep","status":"pending"}],"explanation":""}),
    )
    .await
    .unwrap();
    let output = invoke(
        &tool,
        json!({"items":[{"step":" ","status":"pending"}],"explanation":""}),
    )
    .await
    .unwrap();
    assert_eq!(
        output.as_fail(),
        Some(
            "Plan steps cannot be empty. Please update the plan properly\n\n# Current plan\n# Current Plan\n1. [pending] Keep"
        )
    );
    let output = invoke(&tool, json!({"items":[{"step":"One","status":"in_progress"},{"step":"Two","status":"in_progress"}],"explanation":""})).await.unwrap();
    assert!(
        output.as_fail().unwrap().starts_with(
            "Only one step may be marked in progress. Please update the plan properly"
        )
    );
    let output = invoke(
        &tool,
        json!({"items":[{"step":"One\nTwo","status":"pending"}],"explanation":""}),
    )
    .await
    .unwrap();
    assert_eq!(
        output.as_text(),
        Some("# Current Plan\n1. [pending] One\nTwo")
    );
}

#[test]
fn plan_schema_preserves_the_source_macro_contract_without_public_docs() {
    let schema =
        serde_json::to_value(UpdatePlan::default().tool_spec().parameters_schema.unwrap()).unwrap();
    assert_eq!(
        schema,
        json!({
            "$schema":"https://json-schema.org/draft/2020-12/schema",
            "title":"UpdatePlanArgs", "type":"object", "additionalProperties":false,
            "properties":{
                "items":{"description":"Array of plan steps with `step` and a status (pending, in_progress, completed). Call with an empty list to retrieve the current plan.","type":"array","items":{"$ref":"#/$defs/PlanItem"}},
                "explanation":{"description":"Optional explanation describing how the plan changed.","type":"string"}
            },
            "required":["items","explanation"],
            "$defs":{
                "PlanItem":{"type":"object","properties":{"step":{"type":"string"},"status":{"$ref":"#/$defs/PlanStatus"}},"required":["step","status"],"additionalProperties":false},
                "PlanStatus":{"type":"string","enum":["pending","in_progress","completed"]}
            }
        })
    );
}
