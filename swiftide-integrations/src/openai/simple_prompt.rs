//! Simple prompts use the same completion path as chat and structured prompts.

use async_trait::async_trait;
use swiftide_core::{
    SimplePrompt,
    chat_completion::{ChatCompletionRequest, ChatMessage, errors::LanguageModelError},
    prompt::Prompt,
};

use super::GenericOpenAI;

#[async_trait]
impl<C> SimplePrompt for GenericOpenAI<C>
where
    C: async_openai::config::Config + Default + Sync + Send + std::fmt::Debug + Clone + 'static,
{
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

#[allow(clippy::items_after_statements)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::OpenAI;
    use serde_json::Value;
    use wiremock::{
        Mock, MockServer, Request, Respond, ResponseTemplate,
        matchers::{method, path},
    };

    #[test_log::test(tokio::test)]
    async fn test_prompt_errors_when_model_missing() {
        let openai = OpenAI::builder().build().unwrap();
        let result = openai.prompt("hello".into()).await;
        assert!(matches!(result, Err(LanguageModelError::PermanentError(_))));
    }

    #[test_log::test(tokio::test)]
    async fn test_prompt_via_responses_api_returns_message() {
        let mock_server = MockServer::start().await;

        let response_body = serde_json::json!({
            "created_at": 0,
            "id": "resp",
            "model": "gpt-4.1-mini",
            "object": "response",
            "status": "completed",
            "output": [
                {
                    "type": "message",
                    "id": "msg",
                    "role": "assistant",
                    "status": "completed",
                    "content": [
                        {"type": "output_text", "text": "Hello world", "annotations": []}
                    ]
                }
            ],
            "usage": {
                "input_tokens": 4,
                "input_tokens_details": {"cached_tokens": 0},
                "output_tokens": 2,
                "output_tokens_details": {"reasoning_tokens": 0},
                "total_tokens": 6
            }
        });

        struct ValidatePromptRequest {
            response: Value,
        }

        impl Respond for ValidatePromptRequest {
            fn respond(&self, request: &Request) -> ResponseTemplate {
                let payload: Value = serde_json::from_slice(&request.body).unwrap();
                assert_eq!(payload["model"], self.response["model"]);
                let items = payload["input"].as_array().expect("array input");
                assert_eq!(items.len(), 1);
                assert_eq!(items[0]["type"], "message");
                ResponseTemplate::new(200).set_body_json(self.response.clone())
            }
        }

        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ValidatePromptRequest {
                response: response_body,
            })
            .mount(&mock_server)
            .await;

        let config = async_openai::config::OpenAIConfig::new().with_api_base(mock_server.uri());
        let client = async_openai::Client::with_config(config);

        let openai = OpenAI::builder()
            .client(client)
            .default_prompt_model("gpt-4.1-mini")
            .use_responses_api(true)
            .build()
            .unwrap();

        let result = openai.prompt("Say hi".into()).await.unwrap();
        assert_eq!(result, "Hello world");
    }

    #[test_log::test(tokio::test)]
    async fn test_prompt_via_responses_api_missing_output_errors() {
        let mock_server = MockServer::start().await;
        let empty_response = serde_json::json!({
            "created_at": 0,
            "id": "resp",
            "model": "gpt-4.1-mini",
            "object": "response",
            "output": [],
            "status": "completed"
        });

        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(200).set_body_json(empty_response))
            .mount(&mock_server)
            .await;

        let config = async_openai::config::OpenAIConfig::new().with_api_base(mock_server.uri());
        let client = async_openai::Client::with_config(config);

        let openai = OpenAI::builder()
            .client(client)
            .default_prompt_model("gpt-4.1-mini")
            .use_responses_api(true)
            .build()
            .unwrap();

        let err = openai.prompt("test".into()).await.unwrap_err();
        assert!(matches!(err, LanguageModelError::PermanentError(_)));
    }
}
