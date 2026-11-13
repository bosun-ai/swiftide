//! Structured prompts use the shared completion path with a per-request output schema.

use async_trait::async_trait;
use schemars::Schema;
use swiftide_core::{
    DynStructuredPrompt,
    chat_completion::{ChatCompletionRequest, ChatMessage, errors::LanguageModelError},
    prompt::Prompt,
};

use super::Mistral;

#[async_trait]
impl DynStructuredPrompt for Mistral {
    #[cfg_attr(not(feature = "langfuse"), tracing::instrument(skip_all, err))]
    #[cfg_attr(
        feature = "langfuse",
        tracing::instrument(skip_all, err, fields(langfuse.type = "GENERATION"))
    )]
    async fn structured_prompt_dyn(
        &self,
        prompt: Prompt,
        schema: Schema,
    ) -> Result<serde_json::Value, LanguageModelError> {
        let request = ChatCompletionRequest::from(vec![ChatMessage::new_user(prompt.render()?)]);
        let content = self
            .complete_with_schema(&request, Some(schema))
            .await?
            .message
            .ok_or_else(|| LanguageModelError::permanent("Expected content in response"))?;
        serde_json::from_str(&content).map_err(LanguageModelError::permanent)
    }
}
