use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_bedrockruntime::primitives::Blob;
use swiftide_core::{
    chat_completion::errors::LanguageModelError, indexing::SimplePrompt, prompt::Prompt,
};

use super::AwsBedrock;

#[async_trait]
impl SimplePrompt for AwsBedrock {
    #[tracing::instrument(skip_all, err)]
    async fn prompt(&self, prompt: Prompt) -> Result<String, LanguageModelError> {
        let blob = self
            .model_family
            .build_request_to_bytes(prompt.render()?, &self.model_config)
            .map(Blob::new)?;

        let response_bytes = self.client.prompt_u8(&self.model_id, blob).await?;

        tracing::debug!(
            "Received response: {:?}",
            std::str::from_utf8(&response_bytes).map_err(LanguageModelError::permanent)
        );

        self.model_family
            .output_message_from_bytes(&response_bytes)
            .map_err(std::convert::Into::into)
    }
}

#[cfg(test)]
mod test {
    use crate::aws_bedrock::models::*;
    use crate::aws_bedrock::MockBedrockPrompt;

    use super::*;
    use anyhow::Context as _;
    use test_log;

    #[test_log::test(tokio::test)]
    async fn test_prompt_with_titan() {
        let mut bedrock_mock = MockBedrockPrompt::new();

        bedrock_mock.expect_prompt_u8().once().returning(|_, _| {
            serde_json::to_vec(&TitanResponse {
                input_text_token_count: 1,
                results: vec![TitanTextResult {
                    output_text: "Hello, world!".to_string(),
                    token_count: 1,
                    completion_reason: "STOP".to_string(),
                }],
            })
            .context("Failed to serialize response")
        });

        let bedrock = AwsBedrock::build_titan_family("my_model")
            .test_client(bedrock_mock)
            .build()
            .unwrap();

        let response = bedrock.prompt("Hello".into()).await.unwrap();

        assert_eq!(response, "Hello, world!");
    }

    #[test_log::test(tokio::test)]
    async fn test_prompt_with_anthropic() {
        let mut bedrock_mock = MockBedrockPrompt::new();
        bedrock_mock.expect_prompt_u8().once().returning(|_, _| {
            serde_json::to_vec(&AnthropicResponse {
                content: vec![AnthropicMessageContent {
                    _type: "text".to_string(),
                    text: "Hello, world!".to_string(),
                }],
                id: "id".to_string(),
                model: "model".to_string(),
                _type: "text".to_string(),
                role: "user".to_string(),
                stop_reason: Some("max_tokens".to_string()),
                stop_sequence: None,
                usage: AnthropicUsage {
                    input_tokens: 10,
                    output_tokens: 10,
                },
            })
            .context("Failed to serialize response")
        });
        let bedrock = AwsBedrock::build_anthropic_family("my_model")
            .test_client(bedrock_mock)
            .build()
            .unwrap();
        let response = bedrock.prompt("Hello".into()).await.unwrap();
        assert_eq!(response, "Hello, world!");
    }
}
