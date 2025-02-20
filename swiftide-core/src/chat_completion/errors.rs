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

type LLMError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Error, Debug)]
pub enum ChatCompletionError {
    #[error("Context length exceeded: {0}")]
    ContextLengthExceeded(LLMError),
    #[error("Client error: {0}")]
    ClientError(LLMError),
    #[error("Transient error: {0}")]
    TransientError(LLMError),
}

impl From<LLMError> for ChatCompletionError {
    fn from(e: LLMError) -> Self {
        ChatCompletionError::ClientError(e.into())
    }
}

impl From<anyhow::Error> for ChatCompletionError {
    fn from(e: anyhow::Error) -> Self {
        ChatCompletionError::ClientError(e.into())
    }
}
