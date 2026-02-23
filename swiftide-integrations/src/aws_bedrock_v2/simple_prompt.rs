use async_trait::async_trait;
use swiftide_core::{
    ChatCompletion,
    chat_completion::{ChatCompletionRequest, ChatMessage, errors::LanguageModelError},
    indexing::SimplePrompt,
    prompt::Prompt,
};

#[cfg(test)]
use crate::aws_bedrock_v2::Options;

use super::AwsBedrock;

#[async_trait]
impl SimplePrompt for AwsBedrock {
    #[tracing::instrument(skip_all, err)]
    async fn prompt(&self, prompt: Prompt) -> Result<String, LanguageModelError> {
        let prompt_text = prompt.render()?;
        let request = ChatCompletionRequest::builder()
            .messages(vec![ChatMessage::User(prompt_text)])
            .build()
            .map_err(LanguageModelError::permanent)?;

        let response = self.complete(&request).await?;
        response
            .message
            .ok_or_else(|| LanguageModelError::permanent("No text in response"))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    };

    use aws_sdk_bedrockruntime::{
        operation::converse::ConverseOutput,
        types::{
            ContentBlock, ConversationRole, ConverseOutput as ConverseResult, Message, StopReason,
            TokenUsage,
        },
    };

    use super::*;
    use crate::aws_bedrock_v2::{AwsBedrock, MockBedrockConverse};

    fn response_with_text(text: &str) -> ConverseOutput {
        ConverseOutput::builder()
            .output(ConverseResult::Message(
                Message::builder()
                    .role(ConversationRole::Assistant)
                    .content(ContentBlock::Text(text.to_string()))
                    .build()
                    .unwrap(),
            ))
            .stop_reason(StopReason::EndTurn)
            .build()
            .unwrap()
    }

    #[test_log::test(tokio::test)]
    async fn test_prompt_requires_model() {
        let mut bedrock_mock = MockBedrockConverse::new();
        bedrock_mock.expect_converse().never();

        let bedrock = AwsBedrock::builder()
            .test_client(bedrock_mock)
            .build()
            .unwrap();

        let error = bedrock.prompt("hello".into()).await.unwrap_err();
        assert!(matches!(error, LanguageModelError::PermanentError(_)));
    }

    #[test_log::test(tokio::test)]
    async fn test_prompt_uses_converse_api_and_extracts_text() {
        let mut bedrock_mock = MockBedrockConverse::new();

        bedrock_mock
            .expect_converse()
            .once()
            .withf(|model_id, messages, system, inference_config, tool_config, output_config| {
                model_id == "anthropic.claude-3-5-sonnet-20241022-v2:0"
                    && messages.len() == 1
                    && matches!(messages[0].role(), ConversationRole::User)
                    && matches!(messages[0].content().first(), Some(ContentBlock::Text(text)) if text == "Hello")
                    && system.is_none()
                    && tool_config.is_none()
                    && output_config.is_none()
                    && inference_config
                        .as_ref()
                        .is_some_and(|config| {
                            config.max_tokens() == Some(256)
                                && config.temperature() == Some(0.4)
                                && config.top_p() == Some(0.9)
                                && config.stop_sequences() == ["STOP"]
                        })
            })
            .returning(|_, _, _, _, _, _| Ok(response_with_text("Hello, world!")));

        let bedrock = AwsBedrock::builder()
            .test_client(bedrock_mock)
            .default_prompt_model("anthropic.claude-3-5-sonnet-20241022-v2:0")
            .default_options(Options {
                max_tokens: Some(256),
                temperature: Some(0.4),
                top_p: Some(0.9),
                stop_sequences: Some(vec!["STOP".to_string()]),
                ..Default::default()
            })
            .build()
            .unwrap();

        let response = bedrock.prompt("Hello".into()).await.unwrap();

        assert_eq!(response, "Hello, world!");
    }

    #[test_log::test(tokio::test)]
    async fn test_prompt_maps_context_window_stop_reason() {
        let mut bedrock_mock = MockBedrockConverse::new();

        bedrock_mock
            .expect_converse()
            .once()
            .returning(|_, _, _, _, _, _| {
                Ok(ConverseOutput::builder()
                    .stop_reason(StopReason::ModelContextWindowExceeded)
                    .build()
                    .unwrap())
            });

        let bedrock = AwsBedrock::builder()
            .test_client(bedrock_mock)
            .default_prompt_model("anthropic.claude-3-5-sonnet-20241022-v2:0")
            .build()
            .unwrap();

        let error = bedrock.prompt("Hello".into()).await.unwrap_err();

        assert!(matches!(
            error,
            LanguageModelError::ContextLengthExceeded(_)
        ));
    }

    #[test_log::test(tokio::test)]
    async fn test_prompt_invokes_usage_callback() {
        let mut bedrock_mock = MockBedrockConverse::new();

        bedrock_mock
            .expect_converse()
            .once()
            .returning(|_, _, _, _, _, _| {
                Ok(ConverseOutput::builder()
                    .output(ConverseResult::Message(
                        Message::builder()
                            .role(ConversationRole::Assistant)
                            .content(ContentBlock::Text("ok".to_string()))
                            .build()
                            .unwrap(),
                    ))
                    .usage(
                        TokenUsage::builder()
                            .input_tokens(11)
                            .output_tokens(7)
                            .total_tokens(18)
                            .cache_read_input_tokens(5)
                            .build()
                            .unwrap(),
                    )
                    .stop_reason(StopReason::EndTurn)
                    .build()
                    .unwrap())
            });

        let observed_total = Arc::new(AtomicU32::new(0));
        let observed_total_for_callback = observed_total.clone();

        let bedrock = AwsBedrock::builder()
            .test_client(bedrock_mock)
            .default_prompt_model("anthropic.claude-3-5-sonnet-20241022-v2:0")
            .on_usage(move |usage| {
                observed_total_for_callback.store(usage.total_tokens, Ordering::Relaxed);
                assert_eq!(usage.prompt_tokens, 11);
                assert_eq!(usage.completion_tokens, 7);
                assert_eq!(usage.total_tokens, 18);
                assert_eq!(
                    usage
                        .details
                        .as_ref()
                        .and_then(|details| details.input_tokens_details.as_ref())
                        .and_then(|details| details.cached_tokens),
                    Some(5)
                );

                Ok(())
            })
            .build()
            .unwrap();

        let response = bedrock.prompt("Hello".into()).await.unwrap();

        assert_eq!(response, "ok");
        assert_eq!(observed_total.load(Ordering::Relaxed), 18);
    }
}
