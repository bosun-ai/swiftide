use schemars::{Schema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
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

fn nested_comment_tool_spec() -> ToolSpec {
    ToolSpec::builder()
        .name("create_comment")
        .description("Create a comment")
        .parameters_schema(schema_for!(NestedCommentArgs))
        .build()
        .expect("nested comment tool spec should build")
}

fn nested_request_schema<'a>(schema: &'a Value, defs_name: &str) -> &'a Value {
    let nested_ref = schema["properties"]["request"]["$ref"]
        .as_str()
        .expect("nested request should be a $ref");
    let nested_name = nested_ref
        .rsplit('/')
        .next()
        .expect("nested request ref should have a final path segment");

    &schema[defs_name][nested_name]
}

#[test]
fn canonical_schema_preserves_nested_optional_fields_like_master() {
    let schema = nested_comment_tool_spec()
        .canonical_parameters_schema_json()
        .expect("canonical schema should render");
    let nested_schema = nested_request_schema(&schema, "$defs");

    assert_eq!(schema["required"], json!(["request"]));
    assert_eq!(nested_schema["required"], json!(["body"]));
}

#[test]
fn canonical_schema_preserves_schema_metadata_like_master() {
    let schema = nested_comment_tool_spec()
        .canonical_parameters_schema_json()
        .expect("canonical schema should render");

    assert!(
        schema.get("$schema").is_some(),
        "master preserved the schema metadata in the canonical schema"
    );
}

#[test]
fn toolspec_accepts_non_nullable_one_of_like_master_core_contract() {
    let schema: Schema = serde_json::from_value(json!({
        "type": "object",
        "properties": {
            "content": {
                "oneOf": [
                    { "type": "string" },
                    { "type": "integer" }
                ]
            }
        },
        "required": ["content"]
    }))
    .expect("schema should deserialize");

    ToolSpec::builder()
        .name("one_of_tool")
        .description("Accepts string or integer content")
        .parameters_schema(schema)
        .build()
        .expect("master accepted oneOf at the ToolSpec layer");
}
