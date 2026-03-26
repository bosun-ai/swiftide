use schemars::Schema;
use serde_json::{Map, Value, json};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq)]
pub struct StrictToolParametersSchema {
    document: Value,
}

#[derive(Debug, Error)]
pub enum ToolSchemaError {
    #[error("failed to serialize tool schema")]
    SerializeSchema(#[from] serde_json::Error),
    #[error("tool schema must be a JSON object")]
    RootMustBeObject,
    #[error("tool schema node at {path} must be a JSON object")]
    NodeMustBeObject { path: String },
    #[error("tool schema map at {path} must be a JSON object")]
    NodeMapMustBeObject { path: String },
    #[error("tool schema required must be an array at {path}")]
    RequiredMustBeArray { path: String },
    #[error(
        "strict tool schemas do not support patternProperties at {path}; define explicit properties instead"
    )]
    PatternPropertiesUnsupported { path: String },
    #[error(
        "strict tool schemas do not support propertyNames at {path}; define explicit properties instead"
    )]
    PropertyNamesUnsupported { path: String },
    #[error(
        "strict tool schemas do not support open object schemas at {path}; define explicit properties instead"
    )]
    OpenObjectUnsupported { path: String },
    #[error(
        "strict tool schemas do not support schema-valued additionalProperties at {path}; define explicit properties instead"
    )]
    SchemaValuedAdditionalPropertiesUnsupported { path: String },
    #[error("strict tool schemas do not support {kind}-valued additionalProperties at {path}")]
    InvalidAdditionalProperties { path: String, kind: &'static str },
    #[error("strict tool schemas do not support $ref siblings {keywords} at {path}")]
    UnsupportedRefSiblingKeywords { path: String, keywords: String },
}

impl StrictToolParametersSchema {
    pub(super) fn try_from_raw(schema: Option<&Schema>) -> Result<Self, ToolSchemaError> {
        let raw = match schema {
            Some(schema) => serde_json::to_value(schema)?,
            None => json!({}),
        };

        let root = raw.as_object().ok_or(ToolSchemaError::RootMustBeObject)?;

        Ok(Self {
            document: Value::Object(parse_schema_object(root, &SchemaPath::root(), true)?),
        })
    }

    pub fn into_json(self) -> Value {
        self.document
    }

    pub fn as_json(&self) -> &Value {
        &self.document
    }
}

fn parse_schema_value(value: &Value, path: &SchemaPath) -> Result<Value, ToolSchemaError> {
    let object = value
        .as_object()
        .ok_or_else(|| ToolSchemaError::NodeMustBeObject {
            path: path.to_string(),
        })?;

    Ok(Value::Object(parse_schema_object(object, path, false)?))
}

fn parse_schema_object(
    schema: &Map<String, Value>,
    path: &SchemaPath,
    force_object: bool,
) -> Result<Map<String, Value>, ToolSchemaError> {
    let schema = normalize_schema_object(schema, path)?;

    if force_object || schema_is_object(&schema) {
        parse_object_schema(&schema, path)
    } else {
        parse_non_object_schema(&schema, path)
    }
}

fn normalize_schema_object(
    schema: &Map<String, Value>,
    path: &SchemaPath,
) -> Result<Map<String, Value>, ToolSchemaError> {
    let mut normalized = schema.clone();
    rewrite_nullable_type_union(&mut normalized);
    rewrite_nullable_one_of(&mut normalized);
    strip_ref_annotation_siblings(&mut normalized, path)?;
    Ok(normalized)
}

fn rewrite_nullable_type_union(schema: &mut Map<String, Value>) {
    let Some(entries) = schema.get("type").and_then(Value::as_array) else {
        return;
    };

    let Some(non_null_type) = nullable_type_union(entries).map(str::to_owned) else {
        return;
    };

    let mut non_null_branch = schema.clone();
    non_null_branch.insert("type".to_string(), Value::String(non_null_type));
    let annotations = extract_schema_annotations(schema);

    for key in schema_annotation_keys() {
        non_null_branch.remove(*key);
    }

    schema.clear();
    schema.extend(annotations);
    schema.insert(
        "anyOf".to_string(),
        Value::Array(vec![
            Value::Object(non_null_branch),
            json!({ "type": "null" }),
        ]),
    );
}

fn rewrite_nullable_one_of(schema: &mut Map<String, Value>) {
    let Some(entries) = schema.get("oneOf").and_then(Value::as_array).cloned() else {
        return;
    };

    if is_nullable_union(&entries) {
        schema.remove("oneOf");
        schema.insert("anyOf".to_string(), Value::Array(entries));
    }
}

fn is_nullable_union(entries: &[Value]) -> bool {
    entries.len() == 2 && entries.iter().any(is_null_schema)
}

fn nullable_type_union(entries: &[Value]) -> Option<&str> {
    if entries.len() != 2 {
        return None;
    }

    let mut non_null = None;

    for entry in entries {
        let kind = entry.as_str()?;
        if kind == "null" {
            continue;
        }

        if non_null.is_some() {
            return None;
        }

        non_null = Some(kind);
    }

    non_null
}

fn is_null_schema(value: &Value) -> bool {
    matches!(
        value,
        Value::Object(object) if matches!(object.get("type"), Some(Value::String(kind)) if kind == "null")
    )
}

fn extract_schema_annotations(schema: &Map<String, Value>) -> Map<String, Value> {
    schema_annotation_keys()
        .iter()
        .filter_map(|key| {
            schema
                .get(*key)
                .cloned()
                .map(|value| ((*key).to_string(), value))
        })
        .collect()
}

fn schema_annotation_keys() -> &'static [&'static str] {
    &[
        "description",
        "title",
        "default",
        "examples",
        "deprecated",
        "readOnly",
        "writeOnly",
    ]
}

fn strip_ref_annotation_siblings(
    schema: &mut Map<String, Value>,
    path: &SchemaPath,
) -> Result<(), ToolSchemaError> {
    const SAFE_REF_ANNOTATIONS: &[&str] = &[
        "description",
        "title",
        "default",
        "examples",
        "deprecated",
        "readOnly",
        "writeOnly",
    ];

    if !schema.contains_key("$ref") {
        return Ok(());
    }

    let mut unsupported = Vec::new();
    let sibling_keys = schema
        .keys()
        .filter(|key| key.as_str() != "$ref")
        .cloned()
        .collect::<Vec<_>>();

    for key in sibling_keys {
        if SAFE_REF_ANNOTATIONS.contains(&key.as_str()) {
            schema.remove(&key);
        } else {
            unsupported.push(key);
        }
    }

    if unsupported.is_empty() {
        Ok(())
    } else {
        Err(ToolSchemaError::UnsupportedRefSiblingKeywords {
            path: path.to_string(),
            keywords: unsupported.join(", "),
        })
    }
}

fn parse_object_schema(
    schema: &Map<String, Value>,
    path: &SchemaPath,
) -> Result<Map<String, Value>, ToolSchemaError> {
    if schema.get("patternProperties").is_some() {
        return Err(ToolSchemaError::PatternPropertiesUnsupported {
            path: path.to_string(),
        });
    }

    if schema.get("propertyNames").is_some() {
        return Err(ToolSchemaError::PropertyNamesUnsupported {
            path: path.to_string(),
        });
    }

    match schema.get("additionalProperties") {
        Some(Value::Bool(true)) => {
            return Err(ToolSchemaError::OpenObjectUnsupported {
                path: path.to_string(),
            });
        }
        Some(Value::Object(_)) => {
            return Err(
                ToolSchemaError::SchemaValuedAdditionalPropertiesUnsupported {
                    path: path.to_string(),
                },
            );
        }
        Some(Value::Array(_)) => {
            return Err(ToolSchemaError::InvalidAdditionalProperties {
                path: path.to_string(),
                kind: "array",
            });
        }
        Some(Value::Null) => {
            return Err(ToolSchemaError::InvalidAdditionalProperties {
                path: path.to_string(),
                kind: "null",
            });
        }
        Some(Value::String(_) | Value::Number(_)) => {
            return Err(ToolSchemaError::InvalidAdditionalProperties {
                path: path.to_string(),
                kind: "scalar",
            });
        }
        Some(Value::Bool(false)) | None => {}
    }

    let mut parsed = schema.clone();
    parsed.insert("type".to_string(), Value::String("object".to_string()));
    parsed.insert("additionalProperties".to_string(), Value::Bool(false));
    parsed.insert(
        "properties".to_string(),
        Value::Object(parse_schema_map(
            schema.get("properties"),
            &path.with_key("properties"),
        )?),
    );

    if let Some(required) = schema.get("required")
        && !required.is_array()
    {
        return Err(ToolSchemaError::RequiredMustBeArray {
            path: path.with_key("required").to_string(),
        });
    }

    recurse_schema_children(schema, &mut parsed, path)?;
    Ok(parsed)
}

fn parse_non_object_schema(
    schema: &Map<String, Value>,
    path: &SchemaPath,
) -> Result<Map<String, Value>, ToolSchemaError> {
    let mut parsed = schema.clone();
    recurse_schema_children(schema, &mut parsed, path)?;
    Ok(parsed)
}

fn recurse_schema_children(
    source: &Map<String, Value>,
    target: &mut Map<String, Value>,
    path: &SchemaPath,
) -> Result<(), ToolSchemaError> {
    for key in ["items", "contains", "if", "then", "else", "not"] {
        if let Some(schema) = source.get(key) {
            target.insert(
                key.to_string(),
                parse_schema_value(schema, &path.with_key(key))?,
            );
        }
    }

    for key in ["anyOf", "oneOf", "allOf", "prefixItems"] {
        if let Some(entries) = source.get(key).and_then(Value::as_array) {
            target.insert(
                key.to_string(),
                Value::Array(
                    entries
                        .iter()
                        .enumerate()
                        .map(|(index, schema)| {
                            parse_schema_value(schema, &path.with_index(key, index))
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                ),
            );
        }
    }

    for key in ["properties", "$defs", "definitions", "dependentSchemas"] {
        if let Some(entries) = source.get(key) {
            target.insert(
                key.to_string(),
                Value::Object(parse_schema_map(Some(entries), &path.with_key(key))?),
            );
        }
    }

    Ok(())
}

fn parse_schema_map(
    value: Option<&Value>,
    path: &SchemaPath,
) -> Result<Map<String, Value>, ToolSchemaError> {
    let Some(value) = value else {
        return Ok(Map::new());
    };

    let entries = value
        .as_object()
        .ok_or_else(|| ToolSchemaError::NodeMapMustBeObject {
            path: path.to_string(),
        })?;

    let mut parsed = Map::new();
    for (key, schema) in entries {
        parsed.insert(
            key.clone(),
            parse_schema_value(schema, &path.with_key(key))?,
        );
    }

    Ok(parsed)
}

fn schema_is_object(schema: &Map<String, Value>) -> bool {
    type_includes_object(schema.get("type"))
        || schema.contains_key("properties")
        || schema.contains_key("additionalProperties")
        || schema.contains_key("patternProperties")
        || schema.contains_key("propertyNames")
}

fn type_includes_object(value: Option<&Value>) -> bool {
    match value {
        Some(Value::String(kind)) => kind == "object",
        Some(Value::Array(kinds)) => kinds
            .iter()
            .filter_map(Value::as_str)
            .any(|kind| kind == "object"),
        _ => false,
    }
}

#[derive(Clone, Debug)]
pub(super) struct SchemaPath(Vec<String>);

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
    use super::*;

    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct NestedCommentArgs {
        request: NestedCommentRequest,
    }

    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
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

    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(transparent)]
    struct FreeformObject(serde_json::Map<String, Value>);

    impl schemars::JsonSchema for FreeformObject {
        fn schema_name() -> std::borrow::Cow<'static, str> {
            "FreeformObject".into()
        }

        fn json_schema(_generator: &mut schemars::SchemaGenerator) -> Schema {
            serde_json::from_value(json!({
                "type": "object",
                "additionalProperties": true
            }))
            .expect("freeform object schema should serialize")
        }
    }

    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct CreateViewArgs {
        request: CreateViewRequest,
    }

    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct CreateViewRequest {
        body: FreeformObject,
    }

    #[test]
    fn strict_tool_schema_rejects_nested_freeform_object_wrappers() {
        let error =
            StrictToolParametersSchema::try_from_raw(Some(&schemars::schema_for!(CreateViewArgs)))
                .expect_err("freeform object should be rejected in strict mode");

        let message = error.to_string();
        assert!(message.contains("strict tool schemas do not support open object schemas"));
        assert!(message.contains("FreeformObject"));
    }

    #[test]
    fn strict_tool_schema_rewrites_nullable_type_unions_to_any_of() {
        let schema: Schema = serde_json::from_value(json!({
            "type": "object",
            "properties": {
                "body": {
                    "type": ["string", "null"]
                }
            },
            "required": ["body"]
        }))
        .expect("schema should deserialize");

        let rendered = StrictToolParametersSchema::try_from_raw(Some(&schema))
            .unwrap()
            .into_json();

        let body = &rendered["properties"]["body"];
        assert!(body.get("type").is_none());
        assert!(body.get("oneOf").is_none());
        assert_eq!(
            body["anyOf"],
            Value::Array(vec![json!({ "type": "string" }), json!({ "type": "null" })])
        );
    }

    #[test]
    fn strict_tool_schema_rewrites_nullable_one_of_to_any_of() {
        let schema: Schema = serde_json::from_value(json!({
            "type": "object",
            "properties": {
                "body": {
                    "oneOf": [
                        { "type": "string" },
                        { "type": "null" }
                    ]
                }
            },
            "required": ["body"]
        }))
        .expect("schema should deserialize");

        let rendered = StrictToolParametersSchema::try_from_raw(Some(&schema))
            .unwrap()
            .into_json();

        let body = &rendered["properties"]["body"];
        assert!(body.get("type").is_none());
        assert!(body.get("oneOf").is_none());
        assert_eq!(
            body["anyOf"],
            Value::Array(vec![json!({ "type": "string" }), json!({ "type": "null" })])
        );
    }

    #[test]
    fn strict_tool_schema_strips_ref_annotation_siblings() {
        let schema: Schema = serde_json::from_value(json!({
            "type": "object",
            "properties": {
                "request": {
                    "$ref": "#/$defs/NestedCommentRequest",
                    "description": "A nested payload"
                }
            },
            "required": ["request"],
            "$defs": {
                "NestedCommentRequest": {
                    "type": "object",
                    "properties": {
                        "body": { "type": "string" }
                    },
                    "required": ["body"]
                }
            }
        }))
        .expect("schema should deserialize");

        let rendered = StrictToolParametersSchema::try_from_raw(Some(&schema))
            .unwrap()
            .into_json();

        assert_eq!(
            rendered["properties"]["request"],
            json!({ "$ref": "#/$defs/NestedCommentRequest" })
        );
    }

    #[test]
    fn strict_tool_schema_preserves_nullable_numeric_constraints_on_the_non_null_branch() {
        let schema: Schema = serde_json::from_value(json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": {
                "page_size": {
                    "type": ["integer", "null"],
                    "format": "uint",
                    "minimum": 0
                }
            },
            "required": ["page_size"]
        }))
        .expect("schema should deserialize");

        let rendered = StrictToolParametersSchema::try_from_raw(Some(&schema))
            .unwrap()
            .into_json();

        assert_eq!(
            rendered.get("$schema"),
            Some(&json!("https://json-schema.org/draft/2020-12/schema"))
        );
        let page_size = &rendered["properties"]["page_size"];
        assert!(page_size.get("format").is_none());
        assert!(page_size.get("minimum").is_none());
        assert_eq!(
            page_size["anyOf"],
            Value::Array(vec![
                json!({ "type": "integer", "format": "uint", "minimum": 0 }),
                json!({ "type": "null" })
            ])
        );
    }

    #[test]
    fn strict_tool_schema_moves_nullable_array_constraints_into_the_array_branch() {
        let schema: Schema = serde_json::from_value(json!({
            "type": "object",
            "properties": {
                "children": {
                    "type": ["array", "null"],
                    "items": { "type": "string" }
                }
            },
            "required": ["children"]
        }))
        .expect("schema should deserialize");

        let rendered = StrictToolParametersSchema::try_from_raw(Some(&schema))
            .unwrap()
            .into_json();

        let children = &rendered["properties"]["children"];
        assert!(children.get("items").is_none());
        assert_eq!(
            children["anyOf"],
            Value::Array(vec![
                json!({ "type": "array", "items": { "type": "string" } }),
                json!({ "type": "null" })
            ])
        );
    }

    #[test]
    fn strict_tool_schema_preserves_optional_nested_fields_before_provider_shaping() {
        let schema = StrictToolParametersSchema::try_from_raw(Some(&schemars::schema_for!(
            NestedCommentArgs
        )))
        .unwrap();

        let rendered = schema.into_json();
        let nested_ref = rendered["properties"]["request"]["$ref"]
            .as_str()
            .expect("nested request should be referenced");
        let nested_name = nested_ref
            .rsplit('/')
            .next()
            .expect("nested request ref name");

        assert_eq!(rendered["additionalProperties"], Value::Bool(false));
        assert!(
            rendered["$defs"][nested_name].get("required").is_none(),
            "provider-neutral parsing should not force optional nested fields into required"
        );
    }
}
