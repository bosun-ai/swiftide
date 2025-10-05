use async_trait::async_trait;

use swiftide_core::{
    EmbeddingModel, Embeddings,
    chat_completion::{Usage, errors::LanguageModelError},
};

use super::GenericOpenAI;
use crate::openai::openai_error_to_language_model_error;

#[async_trait]
impl<
    C: async_openai::config::Config + std::default::Default + Sync + Send + std::fmt::Debug + Clone,
> EmbeddingModel for GenericOpenAI<C>
{
    async fn embed(&self, input: Vec<String>) -> Result<Embeddings, LanguageModelError> {
        let model = self
            .default_options
            .embed_model
            .as_ref()
            .ok_or(LanguageModelError::PermanentError("Model not set".into()))?;

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
            .create(request.clone())
            .await
            .map_err(openai_error_to_language_model_error)?;

        let usage = Usage {
            prompt_tokens: response.usage.prompt_tokens,
            completion_tokens: 0,
            total_tokens: response.usage.total_tokens,
        };

        self.track_completion(model, Some(&usage), Some(&request), Some(&response))
            .await?;

        let num_embeddings = response.data.len();
        tracing::debug!(num_embeddings = num_embeddings, "[Embed] Response openai");

        // WARN: Naively assumes that the order is preserved. Might not always be the case.
        Ok(response.data.into_iter().map(|d| d.embedding).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::OpenAI;
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, Request, Respond, ResponseTemplate,
        matchers::{method, path},
    };

    #[test_log::test(tokio::test)]
    async fn test_embed_returns_error_when_model_missing() {
        let openai = OpenAI::builder().build().unwrap();
        let err = openai.embed(vec!["text".into()]).await.unwrap_err();
        assert!(matches!(err, LanguageModelError::PermanentError(_)));
    }

    #[test_log::test(tokio::test)]
    async fn test_embed_success() {
        let mock_server = MockServer::start().await;

        let response_body = json!({
            "data": [{
                "embedding": [0.1, 0.2],
                "index": 0,
                "object": "embedding"
            }],
            "model": "text-embedding-3-small",
            "object": "list",
            "usage": {"prompt_tokens": 5, "total_tokens": 5}
        });

        struct ValidateEmbeddingRequest(serde_json::Value);

        impl Respond for ValidateEmbeddingRequest {
            fn respond(&self, request: &Request) -> ResponseTemplate {
                let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                assert_eq!(body["model"], "text-embedding-3-small");
                assert!(body["input"].is_array());
                ResponseTemplate::new(200).set_body_json(self.0.clone())
            }
        }

        Mock::given(method("POST"))
            .and(path("/embeddings"))
            .respond_with(ValidateEmbeddingRequest(response_body))
            .mount(&mock_server)
            .await;

        let config = async_openai::config::OpenAIConfig::new().with_api_base(mock_server.uri());
        let client = async_openai::Client::with_config(config);

        let openai = OpenAI::builder()
            .client(client)
            .default_embed_model("text-embedding-3-small")
            .build()
            .unwrap();

        let embeddings = openai
            .embed(vec!["Hello".into(), "World".into()])
            .await
            .unwrap();

        assert_eq!(embeddings.len(), 1);
        assert_eq!(embeddings[0], vec![0.1, 0.2]);
    }
}
