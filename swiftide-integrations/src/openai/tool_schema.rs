use serde_json::{Map, Value};
use swiftide_core::chat_completion::{StrictToolParametersSchema, ToolSpec, ToolSpecError};
use thiserror::Error;

type SchemaNormalizer = fn(&mut Value) -> Result<(), OpenAiToolSchemaError>;
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
        let strict_schema = spec.strict_parameters_schema()?;
        let value = OpenAiSchemaPipeline::apply(strict_schema)?;
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
    fn apply(strict_schema: StrictToolParametersSchema) -> Result<Value, OpenAiToolSchemaError> {
        let mut schema = strict_schema.into_json();

        for normalizer in [
            strip_schema_metadata as SchemaNormalizer,
            strip_rust_numeric_formats,
            complete_required_arrays,
        ] {
            normalizer(&mut schema)?;
        }

        {
            let validator = validate_openai_compatibility as SchemaValidator;
            validator(&schema)?;
        }

        Ok(schema)
    }
}

fn strip_schema_metadata(schema: &mut Value) -> Result<(), OpenAiToolSchemaError> {
    walk_schema_mut(schema, &SchemaPath::root(), &mut |node, _| {
        node.remove("$schema");
        Ok(())
    })
}

fn strip_rust_numeric_formats(schema: &mut Value) -> Result<(), OpenAiToolSchemaError> {
    walk_schema_mut(schema, &SchemaPath::root(), &mut |node, _| {
        let should_strip = node
            .get("format")
            .and_then(Value::as_str)
            .is_some_and(is_rust_numeric_format);

        if should_strip {
            node.remove("format");
        }

        Ok(())
    })
}

fn complete_required_arrays(schema: &mut Value) -> Result<(), OpenAiToolSchemaError> {
    walk_schema_mut(schema, &SchemaPath::root(), &mut |node, _| {
        let Some(properties) = node.get("properties").and_then(Value::as_object) else {
            return Ok(());
        };

        node.insert(
            "required".to_string(),
            Value::Array(properties.keys().cloned().map(Value::String).collect()),
        );

        Ok(())
    })
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

fn is_rust_numeric_format(format: &str) -> bool {
    matches!(
        format,
        "int8"
            | "int16"
            | "int32"
            | "int64"
            | "int128"
            | "isize"
            | "uint"
            | "uint8"
            | "uint16"
            | "uint32"
            | "uint64"
            | "uint128"
            | "usize"
    )
}

fn walk_schema_mut(
    value: &mut Value,
    path: &SchemaPath,
    visitor: &mut impl FnMut(&mut Map<String, Value>, &SchemaPath) -> Result<(), OpenAiToolSchemaError>,
) -> Result<(), OpenAiToolSchemaError> {
    let Value::Object(node) = value else {
        return Ok(());
    };

    visitor(node, path)?;
    walk_schema_children_mut(node, path, visitor)
}

fn walk_schema_children_mut(
    node: &mut Map<String, Value>,
    path: &SchemaPath,
    visitor: &mut impl FnMut(&mut Map<String, Value>, &SchemaPath) -> Result<(), OpenAiToolSchemaError>,
) -> Result<(), OpenAiToolSchemaError> {
    for key in ["items", "contains", "if", "then", "else", "not"] {
        if let Some(child) = node.get_mut(key) {
            walk_schema_mut(child, &path.with_key(key), visitor)?;
        }
    }

    for key in ["anyOf", "oneOf", "allOf", "prefixItems"] {
        let Some(entries) = node.get_mut(key).and_then(Value::as_array_mut) else {
            continue;
        };

        for (index, child) in entries.iter_mut().enumerate() {
            walk_schema_mut(child, &path.with_index(key, index), visitor)?;
        }
    }

    for key in ["properties", "$defs", "definitions", "dependentSchemas"] {
        let Some(entries) = node.get_mut(key).and_then(Value::as_object_mut) else {
            continue;
        };

        for (entry_key, child) in entries.iter_mut() {
            walk_schema_mut(child, &path.with_key(key).with_key(entry_key), visitor)?;
        }
    }

    Ok(())
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
    fn openai_tool_schema_adds_recursive_required_arrays() {
        let spec = ToolSpec::builder()
            .name("comment")
            .description("Create a comment")
            .parameters_schema(schemars::schema_for!(NestedCommentArgs))
            .build()
            .unwrap();

        let schema = OpenAiToolSchema::try_from(&spec).unwrap().into_value();
        let nested_ref = schema["properties"]["request"]["$ref"]
            .as_str()
            .expect("nested request should be referenced");
        let nested_name = nested_ref
            .rsplit('/')
            .next()
            .expect("nested request ref name");

        assert_eq!(
            schema["$defs"][nested_name]["required"],
            json!(["block_id", "body", "discussion_id", "page_id", "text"])
        );
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
