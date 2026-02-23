use anyhow::Context as _;
use async_trait::async_trait;
use schemars::Schema;
use swiftide_core::{
    DynStructuredPrompt, SimplePrompt, chat_completion::errors::LanguageModelError, prompt::Prompt,
};

use super::AwsBedrock;

#[async_trait]
impl DynStructuredPrompt for AwsBedrock {
    #[tracing::instrument(skip_all, err)]
    async fn structured_prompt_dyn(
        &self,
        prompt: Prompt,
        schema: Schema,
    ) -> Result<serde_json::Value, LanguageModelError> {
        let prompt_text = prompt.render()?;
        let schema_value = serde_json::to_value(&schema).map_err(LanguageModelError::permanent)?;
        let schema_json =
            serde_json::to_string_pretty(&schema_value).context("Failed to serialize schema")?;

        let constrained_prompt = format!(
            "{prompt_text}\n\n\
             Return ONLY valid JSON (no markdown, no prose) that matches this JSON Schema exactly:\n\
             {schema_json}"
        );

        let response_text = self.prompt(constrained_prompt.into()).await?;
        parse_json_response(&response_text)
    }
}

fn parse_json_response(text: &str) -> Result<serde_json::Value, LanguageModelError> {
    let trimmed = text.trim();

    if let Ok(parsed) = serde_json::from_str(trimmed) {
        return Ok(parsed);
    }

    if let Some(stripped) = strip_markdown_code_fence(trimmed)
        && let Ok(parsed) = serde_json::from_str(stripped.trim())
    {
        return Ok(parsed);
    }

    if let Some(candidate) = extract_json_span(trimmed)
        && let Ok(parsed) = serde_json::from_str(candidate)
    {
        return Ok(parsed);
    }

    Err(LanguageModelError::permanent(anyhow::anyhow!(
        "Failed to parse model response as JSON: {trimmed}"
    )))
}

fn strip_markdown_code_fence(input: &str) -> Option<&str> {
    if !input.starts_with("```") {
        return None;
    }

    let (_, rest) = input.split_once('\n')?;
    rest.strip_suffix("```")
}

fn extract_json_span(input: &str) -> Option<&str> {
    let object_span = input.find('{').zip(input.rfind('}'));
    let array_span = input.find('[').zip(input.rfind(']'));

    let span = match (object_span, array_span) {
        (Some(object_span), Some(array_span)) => {
            if object_span.0 <= array_span.0 {
                object_span
            } else {
                array_span
            }
        }
        (Some(object_span), None) => object_span,
        (None, Some(array_span)) => array_span,
        (None, None) => return None,
    };

    (span.0 <= span.1).then_some(&input[span.0..=span.1])
}

#[cfg(test)]
mod tests {
    use aws_sdk_bedrockruntime::{
        operation::converse::ConverseOutput,
        types::{
            ContentBlock, ConversationRole, ConverseOutput as ConverseResult, Message, StopReason,
        },
    };
    use schemars::{JsonSchema, schema_for};

    use super::*;
    use crate::aws_bedrock_v2::{AwsBedrock, MockBedrockConverse};

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, JsonSchema, PartialEq, Eq)]
    struct StructuredOutput {
        answer: String,
    }

    #[test_log::test(tokio::test)]
    async fn test_structured_prompt_parses_json_response() {
        let mut bedrock_mock = MockBedrockConverse::new();

        bedrock_mock
            .expect_converse()
            .once()
            .withf(|_, messages, _, _, _| {
                messages
                    .first()
                    .and_then(|message| message.content().first())
                    .and_then(|content| content.as_text().ok())
                    .is_some_and(|text| text.contains("JSON Schema exactly"))
            })
            .returning(|_, _, _, _, _| {
                Ok(ConverseOutput::builder()
                    .output(ConverseResult::Message(
                        Message::builder()
                            .role(ConversationRole::Assistant)
                            .content(ContentBlock::Text("{\"answer\":\"42\"}".to_string()))
                            .build()
                            .unwrap(),
                    ))
                    .stop_reason(StopReason::EndTurn)
                    .build()
                    .unwrap())
            });

        let bedrock = AwsBedrock::builder()
            .test_client(bedrock_mock)
            .default_prompt_model("anthropic.claude-3-5-sonnet-20241022-v2:0")
            .build()
            .unwrap();

        let value = bedrock
            .structured_prompt_dyn(
                "What is two times twenty one?".into(),
                schema_for!(StructuredOutput),
            )
            .await
            .unwrap();

        assert_eq!(
            serde_json::from_value::<StructuredOutput>(value).unwrap(),
            StructuredOutput {
                answer: "42".to_string()
            }
        );
    }

    #[test]
    fn test_parse_json_response_accepts_fenced_json() {
        let parsed = parse_json_response("```json\n{\"answer\":\"ok\"}\n```").unwrap();
        assert_eq!(parsed, serde_json::json!({"answer":"ok"}));
    }
}
