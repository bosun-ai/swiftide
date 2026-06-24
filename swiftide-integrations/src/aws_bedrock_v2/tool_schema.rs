use serde_json::{Map, Value};
use swiftide_core::chat_completion::{ToolSpec, ToolSpecError};
use thiserror::Error;

#[derive(Debug)]
pub(super) struct BedrockToolSchema(Value);

impl BedrockToolSchema {
    pub(super) fn try_from_spec(
        spec: &ToolSpec,
        _strict: bool,
    ) -> Result<Self, BedrockToolSchemaError> {
        let mut value = spec.canonical_parameters_schema_json()?;
        strip_schema_metadata(&mut value);
        strip_rust_numeric_formats(&mut value);
        Ok(Self(value))
    }

    pub(super) fn into_value(self) -> Value {
        self.0
    }
}

#[derive(Debug, Error)]
pub(super) enum BedrockToolSchemaError {
    #[error("{0}")]
    InvalidParametersSchema(String),
}

impl From<ToolSpecError> for BedrockToolSchemaError {
    fn from(value: ToolSpecError) -> Self {
        Self::InvalidParametersSchema(value.to_string())
    }
}

fn strip_schema_metadata(schema: &mut Value) {
    walk_schema_mut(schema, &mut |node| {
        node.remove("$schema");
    });
}

fn strip_rust_numeric_formats(schema: &mut Value) {
    walk_schema_mut(schema, &mut |node| {
        let should_strip = node
            .get("format")
            .and_then(Value::as_str)
            .is_some_and(is_rust_numeric_format);

        if should_strip {
            node.remove("format");
        }
    });
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

fn walk_schema_mut(value: &mut Value, visitor: &mut impl FnMut(&mut Map<String, Value>)) {
    let Value::Object(node) = value else {
        return;
    };

    visitor(node);
    walk_schema_children_mut(node, visitor);
}

fn walk_schema_children_mut(
    node: &mut Map<String, Value>,
    visitor: &mut impl FnMut(&mut Map<String, Value>),
) {
    for key in ["items", "contains", "if", "then", "else", "not"] {
        if let Some(child) = node.get_mut(key) {
            walk_schema_mut(child, visitor);
        }
    }

    for key in ["anyOf", "oneOf", "allOf", "prefixItems"] {
        let Some(entries) = node.get_mut(key).and_then(Value::as_array_mut) else {
            continue;
        };

        for child in entries {
            walk_schema_mut(child, visitor);
        }
    }

    for key in ["properties", "$defs", "definitions", "dependentSchemas"] {
        let Some(entries) = node.get_mut(key).and_then(Value::as_object_mut) else {
            continue;
        };

        for child in entries.values_mut() {
            walk_schema_mut(child, visitor);
        }
    }
}
