use thiserror::Error;

use crate::CommandError;

use super::ChatCompletionStream;

/// A `ToolError` is an error that occurs when a tool is invoked.
///
/// Depending on the agent configuration, the tool might be retried with feedback to the LLM, up to
/// a limit.
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

impl ToolError {
    /// Tool received arguments that it could not parse
    pub fn wrong_arguments(e: impl Into<serde_json::Error>) -> Self {
        ToolError::WrongArguments(e.into())
    }

    /// Tool is missing required arguments
    pub fn missing_arguments(tool_name: impl Into<String>) -> Self {
        ToolError::MissingArguments(tool_name.into())
    }

    /// Tool execution failed
    pub fn execution_failed(e: impl Into<CommandError>) -> Self {
        ToolError::ExecutionFailed(e.into())
    }

    /// Tool failed with an unknown error
    pub fn unknown(e: impl Into<anyhow::Error>) -> Self {
        ToolError::Unknown(e.into())
    }
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

// Make it easier to use the error in streaming functions

impl From<LanguageModelError> for ChatCompletionStream {
    fn from(val: LanguageModelError) -> Self {
        Box::pin(futures_util::stream::once(async move { Err(val) }))
    }
}
