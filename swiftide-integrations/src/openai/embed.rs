use async_trait::async_trait;

use swiftide_core::{EmbeddingModel, Embeddings, chat_completion::errors::LanguageModelError};

use super::GenericOpenAI;
use crate::openai::openai_error_to_language_model_error;

#[async_trait]
impl<C: async_openai::config::Config + std::default::Default + Sync + Send + std::fmt::Debug>
    EmbeddingModel for GenericOpenAI<C>
{
    async fn embed(&self, input: Vec<String>) -> Result<Embeddings, LanguageModelError> {
        let model = self
            .default_options
            .embed_model
            .as_ref()
            .ok_or(LanguageModelError::PermanentError("Model not set".into()))?
            .as_ref();

        let request = self
            .embed_request_defaults()
            .model(model)
            .input(&input)
            .build()
            .map_err(LanguageModelError::permanent)?;

        tracing::debug!(
            num_chunks = input.len(),
            model = &model,
            "[Embed] Request to openai"
        );
        let response = self
            .client
            .embeddings()
            .create(request)
            .await
            .map_err(openai_error_to_language_model_error)?;

        let num_embeddings = response.data.len();
        tracing::debug!(num_embeddings = num_embeddings, "[Embed] Response openai");

        // WARN: Naively assumes that the order is preserved. Might not always be the case.
        Ok(response.data.into_iter().map(|d| d.embedding).collect())
    }
}
