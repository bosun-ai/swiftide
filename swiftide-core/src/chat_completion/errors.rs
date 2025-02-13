use thiserror::Error;

use crate::CommandError;

#[derive(Error, Debug)]
pub enum ToolError {
    /// I.e. the llm calls the tool with the wrong arguments
    #[error("arguments for tool failed to parse: {0:#}")]
    WrongArguments(#[from] serde_json::Error),

    /// Tool requires arguments but none were provided
    #[error("arguments missing for tool {0:#}")]
    MissingArguments(String),

    /// Tool execution failed
    #[error("tool execution failed: {0:#}")]
    ExecutionFailed(#[from] CommandError),

    #[error(transparent)]
    Unknown(#[from] anyhow::Error),
}

#[derive(Error, Debug)]
pub enum ChatCompletionError {
    /// Underlying errors by the llm
    #[error("llm returned an error: {0}")]
    LLM(Box<dyn std::error::Error + Send + Sync>),

    #[error(transparent)]
    Unknown(#[from] anyhow::Error),
}
