//! Simple prompts use the same completion path as chat and structured prompts.

use async_trait::async_trait;
use swiftide_core::{
    SimplePrompt,
    chat_completion::{ChatCompletionRequest, ChatMessage, errors::LanguageModelError},
    prompt::Prompt,
};

use super::Mistral;

#[async_trait]
impl SimplePrompt for Mistral {
    #[cfg_attr(not(feature = "langfuse"), tracing::instrument(skip_all, err))]
    #[cfg_attr(
        feature = "langfuse",
        tracing::instrument(skip_all, err, fields(langfuse.type = "GENERATION"))
    )]
    async fn prompt(&self, prompt: Prompt) -> Result<String, LanguageModelError> {
        let request = ChatCompletionRequest::from(vec![ChatMessage::new_user(prompt.render()?)]);
        self.complete_with_schema(&request, None)
            .await?
            .message
            .ok_or_else(|| LanguageModelError::permanent("Expected content in response"))
    }
}
