use async_trait::async_trait;
use aws_sdk_bedrockruntime::types::{
    ContentBlock, ConversationRole, JsonSchemaDefinition, Message, OutputConfig, OutputFormat,
    OutputFormatStructure, OutputFormatType,
};
use schemars::Schema;
#[cfg(feature = "langfuse")]
use serde_json::json;
use swiftide_core::{
    DynStructuredPrompt, chat_completion::errors::LanguageModelError, prompt::Prompt,
};

use super::AwsBedrock;

#[async_trait]
impl DynStructuredPrompt for AwsBedrock {
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
        let prompt_text = prompt.render()?;
        let model = self.prompt_model()?;
        let schema_json = serde_json::to_string(&schema).map_err(LanguageModelError::permanent)?;
        #[cfg(feature = "langfuse")]
        let tracking_request = Some(json!({
            "model": model,
            "prompt": prompt_text.as_str(),
            "schema": schema,
        }));
        #[cfg(not(feature = "langfuse"))]
        let tracking_request: Option<serde_json::Value> = None;

        let message = Message::builder()
            .role(ConversationRole::User)
            .content(ContentBlock::Text(prompt_text))
            .build()
            .map_err(LanguageModelError::permanent)?;

        let output_config = OutputConfig::builder()
            .text_format(
                OutputFormat::builder()
                    .r#type(OutputFormatType::JsonSchema)
                    .structure(OutputFormatStructure::JsonSchema(
                        JsonSchemaDefinition::builder()
                            .schema(schema_json)
                            .name("structured_prompt")
                            .build()
                            .map_err(LanguageModelError::permanent)?,
                    ))
                    .build()
                    .map_err(LanguageModelError::permanent)?,
            )
            .build();

        let response = self
            .client
            .converse(
                model,
                vec![message],
                None,
                super::inference_config_from_options(&self.default_options),
                None,
                Some(output_config),
                self.default_options.additional_model_request_fields.clone(),
                self.default_options
                    .additional_model_response_field_paths
                    .clone(),
            )
            .await?;

        let completion = super::chat_completion::response_to_chat_completion(&response)?;

        self.track_completion(
            model,
            completion.usage.as_ref(),
            tracking_request.as_ref(),
            Some(&completion),
        )
        .await?;

        let Some(response_text) = completion.message else {
            if let Some(error) = super::context_length_exceeded_if_empty(
                false,
                completion.tool_calls.is_some(),
                completion
                    .reasoning
                    .as_ref()
                    .is_some_and(|reasoning| !reasoning.is_empty()),
                Some(response.stop_reason()),
            ) {
                return Err(error);
            }

            return Err(LanguageModelError::permanent("No text in response"));
        };

        serde_json::from_str(response_text.trim()).map_err(|error| {
            LanguageModelError::permanent(anyhow::anyhow!(
                "Failed to parse model response as JSON: {error}"
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    };

    use aws_sdk_bedrockruntime::Client;
    use aws_sdk_bedrockruntime::{
        operation::converse::ConverseOutput,
        types::{
            ContentBlock, ConversationRole, ConverseOutput as ConverseResult, Message, StopReason,
            TokenUsage, ToolUseBlock,
        },
    };
    use aws_smithy_types::Document;
    use schemars::{JsonSchema, schema_for};
    use serde_json::{Value, json};
    use wiremock::{
        Mock, MockServer, Request, Respond, ResponseTemplate,
        matchers::{method, path},
    };

    use super::*;
    use crate::aws_bedrock_v2::{
        AwsBedrock, MockBedrockConverse,
        test_utils::{TEST_MODEL_ID, bedrock_client_for_mock_server},
    };

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
            .withf(
                |_,
                 messages,
                 _,
                 _,
                 _,
                 output_config,
                 _additional_model_request_fields,
                 _additional_model_response_field_paths| {
                    messages
                        .first()
                        .and_then(|message| message.content().first())
                        .and_then(|content| content.as_text().ok())
                        .is_some_and(|text| text == "What is two times twenty one?")
                        && output_config
                            .as_ref()
                            .and_then(|config| config.text_format())
                            .is_some_and(|format| {
                                matches!(format.r#type(), OutputFormatType::JsonSchema)
                                    && format
                                        .structure()
                                        .and_then(|structure| structure.as_json_schema().ok())
                                        .is_some_and(|schema| {
                                            schema.schema().contains("\"answer\"")
                                        })
                            })
                },
            )
            .returning(|_, _, _, _, _, _, _, _| {
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

    #[test_log::test(tokio::test)]
    async fn test_structured_prompt_reports_usage() {
        let mut bedrock_mock = MockBedrockConverse::new();

        bedrock_mock
            .expect_converse()
            .once()
            .returning(|_, _, _, _, _, _, _, _| {
                Ok(ConverseOutput::builder()
                    .output(ConverseResult::Message(
                        Message::builder()
                            .role(ConversationRole::Assistant)
                            .content(ContentBlock::Text("{\"answer\":\"42\"}".to_string()))
                            .build()
                            .unwrap(),
                    ))
                    .usage(
                        TokenUsage::builder()
                            .input_tokens(9)
                            .output_tokens(5)
                            .total_tokens(14)
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
                Ok(())
            })
            .build()
            .unwrap();

        let _ = bedrock
            .structured_prompt_dyn(
                "What is two times twenty one?".into(),
                schema_for!(StructuredOutput),
            )
            .await
            .unwrap();

        assert_eq!(observed_total.load(Ordering::Relaxed), 14);
    }

    #[test_log::test(tokio::test)]
    async fn test_structured_prompt_green_path_with_wiremock() {
        struct ValidateStructuredConverseRequest;

        impl Respond for ValidateStructuredConverseRequest {
            fn respond(&self, request: &Request) -> ResponseTemplate {
                let payload: Value = serde_json::from_slice(&request.body).expect("request json");

                assert_eq!(payload["messages"][0]["role"], "user");
                assert_eq!(
                    payload["messages"][0]["content"][0]["text"],
                    "What is two times twenty one?"
                );
                assert_eq!(payload["outputConfig"]["textFormat"]["type"], "json_schema");
                assert_eq!(
                    payload["outputConfig"]["textFormat"]["structure"]["jsonSchema"]["name"],
                    "structured_prompt"
                );
                let schema =
                    payload["outputConfig"]["textFormat"]["structure"]["jsonSchema"]["schema"]
                        .as_str()
                        .expect("schema string");
                assert!(schema.contains("\"answer\""));

                ResponseTemplate::new(200).set_body_json(json!({
                    "output": {
                        "message": {
                            "role": "assistant",
                            "content": [
                                {"text": "{\"answer\":\"42\"}"}
                            ]
                        }
                    },
                    "stopReason": "end_turn",
                    "usage": {
                        "inputTokens": 2,
                        "outputTokens": 3,
                        "totalTokens": 5
                    }
                }))
            }
        }

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!("/model/{TEST_MODEL_ID}/converse")))
            .respond_with(ValidateStructuredConverseRequest)
            .mount(&mock_server)
            .await;

        let client: Client = bedrock_client_for_mock_server(&mock_server.uri());
        let bedrock = AwsBedrock::builder()
            .client(client)
            .default_prompt_model(TEST_MODEL_ID)
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

    #[test_log::test(tokio::test)]
    async fn test_structured_prompt_returns_error_when_response_has_no_text() {
        let mut bedrock_mock = MockBedrockConverse::new();

        bedrock_mock
            .expect_converse()
            .once()
            .returning(|_, _, _, _, _, _, _, _| {
                Ok(ConverseOutput::builder()
                    .output(ConverseResult::Message(
                        Message::builder()
                            .role(ConversationRole::Assistant)
                            .content(ContentBlock::ToolUse(
                                ToolUseBlock::builder()
                                    .tool_use_id("call_1")
                                    .name("structured_prompt")
                                    .input(Document::Object(HashMap::new()))
                                    .build()
                                    .unwrap(),
                            ))
                            .build()
                            .unwrap(),
                    ))
                    .stop_reason(StopReason::ToolUse)
                    .build()
                    .unwrap())
            });

        let bedrock = AwsBedrock::builder()
            .test_client(bedrock_mock)
            .default_prompt_model("anthropic.claude-3-5-sonnet-20241022-v2:0")
            .build()
            .unwrap();

        let error = bedrock
            .structured_prompt_dyn("Prompt".into(), schema_for!(StructuredOutput))
            .await
            .unwrap_err();
        assert!(matches!(error, LanguageModelError::PermanentError(_)));
        assert!(error.to_string().contains("No text in response"));
    }

    #[test_log::test(tokio::test)]
    async fn test_structured_prompt_returns_error_on_invalid_json_payload() {
        let mut bedrock_mock = MockBedrockConverse::new();

        bedrock_mock
            .expect_converse()
            .once()
            .returning(|_, _, _, _, _, _, _, _| {
                Ok(ConverseOutput::builder()
                    .output(ConverseResult::Message(
                        Message::builder()
                            .role(ConversationRole::Assistant)
                            .content(ContentBlock::Text("not-json".to_string()))
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

        let error = bedrock
            .structured_prompt_dyn("Prompt".into(), schema_for!(StructuredOutput))
            .await
            .unwrap_err();
        assert!(matches!(error, LanguageModelError::PermanentError(_)));
        assert!(
            error
                .to_string()
                .contains("Failed to parse model response as JSON")
        );
    }
}
