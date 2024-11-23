use thiserror::Error;

use super::ToolCall;

#[derive(Error, Debug)]
pub enum ToolError {
    #[error("arguments for tool failed to parse")]
    WrongArguments(ToolCall, serde_json::Error),

    #[error("tool call missing arguments")]
    MissingArguments(ToolCall),

    #[error("tool call failed")]
    ToolFailed(ToolCall, anyhow::Error),

    #[error("unknown tool error")]
    Unknown(ToolCall, anyhow::Error),
}
