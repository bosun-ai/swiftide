use swiftide_core::chat_completion::{
    errors::{ChatCompletionError, ToolError},
    ChatCompletionRequestBuilderError,
};
use thiserror::Error;
use tokio::task::JoinError;

#[derive(Error, Debug)]
pub enum AgentError {
    #[error("Agent is already running")]
    AlreadyRunning,

    #[error("Failed to render system prompt {0}")]
    FailedToRenderSystemPrompt(anyhow::Error),

    #[error("Failed to build chat completion request {0}")]
    FailedToBuildRequest(ChatCompletionRequestBuilderError),

    #[error("Error from LLM when running completions {0}")]
    CompletionsFailed(ChatCompletionError),

    #[error(transparent)]
    ToolError(#[from] ToolError),

    #[error("Failed waiting for tool to finish {0}")]
    ToolFailedToJoin(JoinError),

    #[error("Failed to load tools from toolbox {0}")]
    ToolBoxFailedToLoad(anyhow::Error),
}
