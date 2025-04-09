use anyhow::Context as _;
use async_anthropic::{
    errors::AnthropicError, errors::CreateMessagesError, types::CreateMessagesRequestBuilder,
};
use async_trait::async_trait;
use swiftide_core::{chat_completion::errors::LanguageModelError, indexing::SimplePrompt};

use super::Anthropic;

#[async_trait]
impl SimplePrompt for Anthropic {
    #[tracing::instrument(skip_all, err)]
    async fn prompt(
        &self,
        prompt: swiftide_core::prompt::Prompt,
    ) -> Result<String, LanguageModelError> {
        let model = &self.default_options.prompt_model;

        let request = CreateMessagesRequestBuilder::default()
            .model(model)
            .messages(vec![prompt.render()?.into()])
            .build()
            .map_err(LanguageModelError::permanent)?;

        tracing::debug!(
            model = &model,
            messages =
                serde_json::to_string_pretty(&request).map_err(LanguageModelError::permanent)?,
            "[SimplePrompt] Request to anthropic"
        );

        let response = self.client.messages().create(request).await.map_err(|e| {
            let CreateMessagesError::AnthropicError(e) = e;
            match e {
                AnthropicError::NetworkError(_) => LanguageModelError::TransientError(e.into()),
                // TODO: The Rust Anthropic client is not documented well, we should figure out
                // which of these errors are client errors and which are server errors.
                // And which would be the ContextLengthExceeded error
                // For now, we'll just map all of them to client errors so we get feedback.
                AnthropicError::BadRequest(_)
                | AnthropicError::ApiError(_)
                | AnthropicError::UnexpectedError
                | AnthropicError::Unauthorized
                | AnthropicError::Unknown(_) => LanguageModelError::PermanentError(e.into()),
            }
        })?;

        tracing::debug!(
            response =
                serde_json::to_string_pretty(&response).map_err(LanguageModelError::permanent)?,
            "[SimplePrompt] Response from anthropic"
        );

        let message = response
            .messages()
            .into_iter()
            .next()
            .context("No messages in response")
            .map_err(LanguageModelError::permanent)?;

        message
            .text()
            .context("No text in response")
            .map_err(LanguageModelError::permanent)
    }
}

#[cfg(test)]
mod tests {
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    use super::*;

    #[tokio::test]
    async fn test_simple_prompt_with_mock() {
        // Start a WireMock server
        let mock_server = MockServer::start().await;

        // Create a mock response
        let mock_response = ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{"type": "text", "text": "mocked response"}]
        }));

        // Mock the expected endpoint
        Mock::given(method("POST"))
            .and(path("/v1/messages")) // Adjust path to match expected endpoint
            .respond_with(mock_response)
            .mount(&mock_server)
            .await;

        let client = async_anthropic::Client::builder()
            .base_url(mock_server.uri())
            .build()
            .unwrap();

        // Build an Anthropic client with the mock server's URL
        let mut client_builder = Anthropic::builder();
        client_builder.client(client);
        let client = client_builder.build().unwrap();

        // Call the prompt method
        let result = client.prompt("hello".into()).await.unwrap();

        // Assert the result
        assert_eq!(result, "mocked response");
    }
}
