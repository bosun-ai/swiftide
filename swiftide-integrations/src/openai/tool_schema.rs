use serde_json::{Map, Value};
use swiftide_core::chat_completion::{ToolSpec, ToolSpecError};
use thiserror::Error;

type SchemaValidator = fn(&Value) -> Result<(), OpenAiToolSchemaError>;

#[derive(Debug)]
pub(super) struct OpenAiToolSchema(Value);

impl OpenAiToolSchema {
    pub(super) fn into_value(self) -> Value {
        self.0
    }
}

impl TryFrom<&ToolSpec> for OpenAiToolSchema {
    type Error = OpenAiToolSchemaError;

    fn try_from(spec: &ToolSpec) -> Result<Self, Self::Error> {
        let value = OpenAiSchemaPipeline::apply(spec.canonical_parameters_schema_json()?)?;
        Ok(Self(value))
    }
}

#[derive(Debug, Error)]
pub(super) enum OpenAiToolSchemaError {
    #[error("{0}")]
    InvalidParametersSchema(String),
    #[error("OpenAI strict tool schemas do not support `{keyword}` at {path}")]
    UnsupportedKeyword { path: String, keyword: &'static str },
    #[error("OpenAI strict tool schemas do not support array-valued `type` at {path}")]
    UnsupportedTypeUnion { path: String },
}

impl From<ToolSpecError> for OpenAiToolSchemaError {
    fn from(value: ToolSpecError) -> Self {
        Self::InvalidParametersSchema(value.to_string())
    }
}

struct OpenAiSchemaPipeline;

impl OpenAiSchemaPipeline {
    fn apply(schema: Value) -> Result<Value, OpenAiToolSchemaError> {
        let validator = validate_openai_compatibility as SchemaValidator;
        validator(&schema)?;
        Ok(schema)
    }
}

fn validate_openai_compatibility(schema: &Value) -> Result<(), OpenAiToolSchemaError> {
    walk_schema(schema, &SchemaPath::root(), &mut |node, path| {
        if node.contains_key("oneOf") {
            return Err(OpenAiToolSchemaError::UnsupportedKeyword {
                path: path.to_string(),
                keyword: "oneOf",
            });
        }

        if matches!(node.get("type"), Some(Value::Array(_))) {
            return Err(OpenAiToolSchemaError::UnsupportedTypeUnion {
                path: path.to_string(),
            });
        }

        Ok(())
    })
}

fn walk_schema(
    value: &Value,
    path: &SchemaPath,
    visitor: &mut impl FnMut(&Map<String, Value>, &SchemaPath) -> Result<(), OpenAiToolSchemaError>,
) -> Result<(), OpenAiToolSchemaError> {
    let Value::Object(node) = value else {
        return Ok(());
    };

    visitor(node, path)?;
    walk_schema_children(node, path, visitor)
}

fn walk_schema_children(
    node: &Map<String, Value>,
    path: &SchemaPath,
    visitor: &mut impl FnMut(&Map<String, Value>, &SchemaPath) -> Result<(), OpenAiToolSchemaError>,
) -> Result<(), OpenAiToolSchemaError> {
    for key in ["items", "contains", "if", "then", "else", "not"] {
        if let Some(child) = node.get(key) {
            walk_schema(child, &path.with_key(key), visitor)?;
        }
    }

    for key in ["anyOf", "oneOf", "allOf", "prefixItems"] {
        let Some(entries) = node.get(key).and_then(Value::as_array) else {
            continue;
        };

        for (index, child) in entries.iter().enumerate() {
            walk_schema(child, &path.with_index(key, index), visitor)?;
        }
    }

    for key in ["properties", "$defs", "definitions", "dependentSchemas"] {
        let Some(entries) = node.get(key).and_then(Value::as_object) else {
            continue;
        };

        for (entry_key, child) in entries {
            walk_schema(child, &path.with_key(key).with_key(entry_key), visitor)?;
        }
    }

    Ok(())
}

#[derive(Clone, Debug)]
struct SchemaPath(Vec<String>);

impl SchemaPath {
    fn root() -> Self {
        Self(vec!["$".to_string()])
    }

    fn with_key(&self, key: impl Into<String>) -> Self {
        let mut path = self.0.clone();
        path.push(key.into());
        Self(path)
    }

    fn with_index(&self, key: impl Into<String>, index: usize) -> Self {
        let mut path = self.0.clone();
        path.push(key.into());
        path.push(index.to_string());
        Self(path)
    }
}

impl std::fmt::Display for SchemaPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.join("."))
    }
}

#[cfg(test)]
mod tests {
    use schemars::JsonSchema;
    use serde_json::json;
    use swiftide_core::chat_completion::ToolSpec;

    use super::OpenAiToolSchema;

    #[derive(serde::Serialize, serde::Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct NestedCommentArgs {
        request: NestedCommentRequest,
    }

    #[derive(serde::Serialize, serde::Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct NestedCommentRequest {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        page_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        block_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        discussion_id: Option<String>,
    }

    #[test]
    fn openai_tool_schema_strips_schema_metadata_and_rust_formats() {
        let spec = ToolSpec::builder()
            .name("comment")
            .description("Create a comment")
            .parameters_schema(
                serde_json::from_value::<schemars::Schema>(json!({
                    "$schema": "https://json-schema.org/draft/2020-12/schema",
                    "type": "object",
                    "properties": {
                        "page_size": {
                            "type": ["integer", "null"],
                            "format": "uint",
                            "minimum": 0
                        }
                    }
                }))
                .unwrap(),
            )
            .build()
            .unwrap();

        let schema = OpenAiToolSchema::try_from(&spec).unwrap().into_value();

        assert!(schema.get("$schema").is_none());
        assert_eq!(
            schema["properties"]["page_size"]["anyOf"],
            json!([
                { "type": "integer", "minimum": 0 },
                { "type": "null" }
            ])
        );
    }

    #[test]
    fn openai_tool_schema_uses_core_provider_ready_schema() {
        let spec = ToolSpec::builder()
            .name("comment")
            .description("Create a comment")
            .parameters_schema(schemars::schema_for!(NestedCommentArgs))
            .build()
            .unwrap();

        let schema = OpenAiToolSchema::try_from(&spec).unwrap().into_value();

        assert_eq!(
            schema["properties"]["request"]["required"],
            json!(["block_id", "body", "discussion_id", "page_id", "text"])
        );
        assert!(schema.get("$defs").is_none());
    }

    #[test]
    fn openai_tool_schema_rejects_non_nullable_one_of() {
        let spec = ToolSpec::builder()
            .name("comment")
            .description("Create a comment")
            .parameters_schema(
                serde_json::from_value::<schemars::Schema>(json!({
                    "type": "object",
                    "properties": {
                        "content": {
                            "oneOf": [
                                { "type": "string" },
                                { "type": "integer" }
                            ]
                        }
                    }
                }))
                .unwrap(),
            )
            .build()
            .unwrap();

        let error = OpenAiToolSchema::try_from(&spec).expect_err("oneOf should be rejected");
        assert!(error.to_string().contains("`oneOf`"));
    }
}
