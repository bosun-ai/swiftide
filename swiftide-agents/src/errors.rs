use swiftide_core::chat_completion::{
    ChatCompletionRequestBuilderError,
    errors::{LanguageModelError, ToolError},
};
use thiserror::Error;
use tokio::task::JoinError;

#[derive(Error, Debug)]
pub enum AgentError {
    #[error("Agent is already running")]
    AlreadyRunning,

    #[error("Failed to render system prompt {0:#}")]
    FailedToRenderSystemPrompt(anyhow::Error),

    #[error("Failed to build chat completion request {0:#}")]
    FailedToBuildRequest(ChatCompletionRequestBuilderError),

    #[error("Error from LLM when running completions {0:#}")]
    CompletionsFailed(LanguageModelError),

    #[error(transparent)]
    ToolError(#[from] ToolError),

    #[error("Failed waiting for tool to finish {0:?}")]
    ToolFailedToJoin(String, JoinError),

    #[error("Failed to load tools from toolbox {0:#}")]
    ToolBoxFailedToLoad(anyhow::Error),

    #[error("Chat completion stream was empty")]
    EmptyStream,

    #[error("Failed to render prompt {0:#}")]
    FailedToRenderPrompt(anyhow::Error),

    #[error("Error with message history {0:#}")]
    MessageHistoryError(anyhow::Error),
}
