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
    #[error("Permanent error: {0}")]
    PermanentError(BoxedError),
    #[error("Transient error: {0}")]
    TransientError(BoxedError),
}

impl LanguageModelError {
    pub fn permanent(e: impl Into<BoxedError>) -> Self {
        LanguageModelError::PermanentError(e.into())
    }

    pub fn transient(e: impl Into<BoxedError>) -> Self {
        LanguageModelError::TransientError(e.into())
    }

    pub fn context_length_exceeded(e: impl Into<BoxedError>) -> Self {
        LanguageModelError::ContextLengthExceeded(e.into())
    }
}

impl From<BoxedError> for LanguageModelError {
    fn from(e: BoxedError) -> Self {
        LanguageModelError::PermanentError(e)
    }
}

impl From<anyhow::Error> for LanguageModelError {
    fn from(e: anyhow::Error) -> Self {
        LanguageModelError::PermanentError(e.into())
    }
}
