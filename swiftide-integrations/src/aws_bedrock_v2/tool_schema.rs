use serde_json::Value;
use swiftide_core::chat_completion::{ToolSpec, ToolSpecError};

pub(super) struct AwsBedrockToolSchema(Value);

impl AwsBedrockToolSchema {
    pub(super) fn into_value(self) -> Value {
        self.0
    }
}

impl TryFrom<&ToolSpec> for AwsBedrockToolSchema {
    type Error = ToolSpecError;

    fn try_from(spec: &ToolSpec) -> Result<Self, Self::Error> {
        Ok(Self(spec.strict_parameters_schema()?.into_json()))
    }
}
