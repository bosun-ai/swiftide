//! Structured prompts use the shared completion path with a per-request output schema.

use async_trait::async_trait;
use schemars::Schema;
use swiftide_core::{
    DynStructuredPrompt,
    chat_completion::{ChatCompletionRequest, ChatMessage, errors::LanguageModelError},
    prompt::Prompt,
};

use super::GenericOpenAI;

#[async_trait]
impl<C> DynStructuredPrompt for GenericOpenAI<C>
where
    C: async_openai::config::Config + Default + Sync + Send + std::fmt::Debug + Clone + 'static,
{
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

#[cfg(test)]
mod tests {
    use crate::openai::{self, OpenAI};
    use swiftide_core::StructuredPrompt;

    use super::*;
    use async_openai::Client;
    use async_openai::config::OpenAIConfig;
    use schemars::{JsonSchema, schema_for};
    use serde::{Deserialize, Serialize};
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
    struct SimpleOutput {
        answer: String,
    }

    async fn setup_client() -> (MockServer, OpenAI) {
        // Start the Wiremock server
        let mock_server = MockServer::start().await;

        // Prepare the response the mock should return
        let assistant_msg = serde_json::json!({
            "role": "assistant",
            "content": serde_json::to_string(&SimpleOutput {
                answer: "42".to_owned()
            }).unwrap(),
        });

        let body = serde_json::json!({
          "id": "chatcmpl-B9MBs8CjcvOU2jLn4n570S5qMJKcT",
          "object": "chat.completion",
          "created": 123,
          "model": "gpt-4.1-2025-04-14",
          "choices": [
            {
              "index": 0,
              "message": assistant_msg,
              "logprobs": null,
              "finish_reason": "stop"
            }
          ],
          "usage": {
            "prompt_tokens": 19,
            "completion_tokens": 10,
            "total_tokens": 29,
            "prompt_tokens_details": {
              "cached_tokens": 0,
              "audio_tokens": 0
            },
            "completion_tokens_details": {
              "reasoning_tokens": 0,
              "audio_tokens": 0,
              "accepted_prediction_tokens": 0,
              "rejected_prediction_tokens": 0
            }
          },
          "service_tier": "default"
        });

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&mock_server)
            .await;

        // Point our client at the mock server
        let config = OpenAIConfig::new().with_api_base(mock_server.uri());
        let client = Client::with_config(config);

        // Construct the GenericOpenAI instance
        let opts = openai::Options {
            prompt_model: Some("gpt-4".to_string()),
            ..openai::Options::default()
        };
        (
            mock_server,
            OpenAI::builder()
                .client(client)
                .default_options(opts)
                .build()
                .unwrap(),
        )
    }

    #[tokio::test]
    async fn test_structured_prompt_with_wiremock() {
        let (_guard, ai) = setup_client().await;
        // Call structured_prompt
        let result: serde_json::Value = ai.structured_prompt("test".into()).await.unwrap();
        dbg!(&result);

        // Assert
        assert_eq!(
            serde_json::from_value::<SimpleOutput>(result).unwrap(),
            SimpleOutput {
                answer: "42".into()
            }
        );
    }

    #[tokio::test]
    async fn test_structured_prompt_with_wiremock_as_box() {
        let (_guard, ai) = setup_client().await;
        // Call structured_prompt
        let ai: Box<dyn DynStructuredPrompt> = Box::new(ai);
        let result: serde_json::Value = ai
            .structured_prompt_dyn("test".into(), schema_for!(SimpleOutput))
            .await
            .unwrap();
        dbg!(&result);

        // Assert
        assert_eq!(
            serde_json::from_value::<SimpleOutput>(result).unwrap(),
            SimpleOutput {
                answer: "42".into()
            }
        );
    }

    #[test_log::test(tokio::test)]
    async fn test_structured_prompt_via_responses_api() {
        let mock_server = MockServer::start().await;

        let response_body = json!({
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
                        {"type": "output_text", "text": serde_json::to_string(&SimpleOutput { answer: "structured".into() }).unwrap(), "annotations": []}
                    ]
                }
            ],
            "usage": {
                "input_tokens": 10,
                "input_tokens_details": {"cached_tokens": 0},
                "output_tokens": 4,
                "output_tokens_details": {"reasoning_tokens": 0},
                "total_tokens": 14
            }
        });

        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
            .mount(&mock_server)
            .await;

        let config = OpenAIConfig::new().with_api_base(mock_server.uri());
        let client = Client::with_config(config);

        let openai = OpenAI::builder()
            .client(client)
            .default_prompt_model("gpt-4.1-mini")
            .use_responses_api(true)
            .build()
            .unwrap();

        let schema = schema_for!(SimpleOutput);
        let result = openai
            .structured_prompt_dyn("Render".into(), schema)
            .await
            .unwrap();

        assert_eq!(
            serde_json::from_value::<SimpleOutput>(result).unwrap(),
            SimpleOutput {
                answer: "structured".into(),
            }
        );
    }

    #[test_log::test(tokio::test)]
    async fn test_structured_prompt_via_responses_api_invalid_json_errors() {
        let mock_server = MockServer::start().await;

        let bad_response = json!({
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
                        {"type": "output_text", "text": "not json", "annotations": []}
                    ]
                }
            ]
        });

        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(200).set_body_json(bad_response))
            .mount(&mock_server)
            .await;

        let config = OpenAIConfig::new().with_api_base(mock_server.uri());
        let client = Client::with_config(config);

        let openai = OpenAI::builder()
            .client(client)
            .default_prompt_model("gpt-4.1-mini")
            .use_responses_api(true)
            .build()
            .unwrap();

        let schema = schema_for!(SimpleOutput);
        let err = openai
            .structured_prompt_dyn("Render".into(), schema)
            .await
            .unwrap_err();

        assert!(matches!(err, LanguageModelError::PermanentError(_)));
    }
}
