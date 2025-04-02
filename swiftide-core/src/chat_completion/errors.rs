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

type BoxedError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Error, Debug)]
pub enum LanguageModelError {
    #[error("Context length exceeded: {0}")]
    ContextLengthExceeded(BoxedError),
    #[error("Client error: {0}")]
    ClientError(BoxedError),
    #[error("Transient error: {0}")]
    TransientError(BoxedError),
}

impl From<BoxedError> for LanguageModelError {
    fn from(e: BoxedError) -> Self {
        LanguageModelError::ClientError(e)
    }
}

impl From<anyhow::Error> for LanguageModelError {
    fn from(e: anyhow::Error) -> Self {
        LanguageModelError::ClientError(e.into())
    }
}
