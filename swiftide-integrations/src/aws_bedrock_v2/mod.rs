use std::{pin::Pin, sync::Arc};

use async_trait::async_trait;
use aws_sdk_bedrockruntime::{
    Client,
    error::SdkError,
    operation::{
        converse::{ConverseError, ConverseOutput},
        converse_stream::{
            ConverseStreamError, ConverseStreamOutput as BedrockConverseStreamOutput,
        },
    },
    types::{
        InferenceConfiguration, Message, OutputConfig, StopReason, SystemContentBlock, TokenUsage,
        ToolConfiguration, error::ConverseStreamOutputError,
    },
};
use aws_smithy_types::Document;
use derive_builder::Builder;
use serde::Serialize;
use swiftide_core::chat_completion::{
    InputTokenDetails, Usage, UsageDetails, errors::LanguageModelError,
};
use tokio::runtime::Handle;

#[cfg(test)]
use mockall::automock;

mod chat_completion;
mod simple_prompt;
mod structured_prompt;
#[cfg(test)]
mod test_utils;

/// Converse-based integration with AWS Bedrock.
///
/// This integration uses Bedrock's unified Converse APIs (`Converse` + `ConverseStream`).
#[derive(Builder, Clone)]
#[builder(setter(into, strip_option))]
pub struct AwsBedrock {
    /// The Bedrock runtime client.
    #[builder(default = self.default_client(), setter(custom))]
    client: Arc<dyn BedrockConverse>,

    /// Default options for prompt requests.
    #[builder(default, setter(custom))]
    default_options: Options,

    #[cfg(feature = "metrics")]
    #[builder(default)]
    /// Optional metadata to attach to metrics emitted by this client.
    metric_metadata: Option<std::collections::HashMap<String, String>>,

    /// A callback function that is called when usage information is available.
    #[builder(default, setter(custom))]
    #[allow(clippy::type_complexity)]
    on_usage: Option<
        Arc<
            dyn for<'a> Fn(
                    &'a Usage,
                ) -> Pin<
                    Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>,
                > + Send
                + Sync,
        >,
    >,
}

impl std::fmt::Debug for AwsBedrock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AwsBedrock")
            .field("client", &self.client)
            .field("default_options", &self.default_options)
            .finish()
    }
}

#[derive(Debug, Clone, Builder, Default)]
#[builder(setter(strip_option))]
pub struct Options {
    /// Model ID or ARN used as `modelId` in Converse requests.
    #[builder(default, setter(into))]
    pub prompt_model: Option<String>,

    /// Maximum number of tokens in the generated response.
    #[builder(default)]
    pub max_tokens: Option<i32>,

    /// Sampling temperature.
    #[builder(default)]
    pub temperature: Option<f32>,

    /// Nucleus sampling parameter.
    #[builder(default)]
    pub top_p: Option<f32>,

    /// Stop sequences for response generation.
    #[builder(default, setter(into))]
    pub stop_sequences: Option<Vec<String>>,

    /// Whether tool calls should enforce strict schema validation.
    ///
    /// Defaults to `true` when not set.
    #[builder(default)]
    pub tool_strict: Option<bool>,

    /// Provider-specific model request parameters passed to Converse.
    ///
    /// This is the Bedrock equivalent of model-specific reasoning controls.
    #[builder(default)]
    pub additional_model_request_fields: Option<Document>,

    /// JSON Pointer paths for model-specific response fields.
    #[builder(default, setter(into))]
    pub additional_model_response_field_paths: Option<Vec<String>>,
}

impl Options {
    pub fn builder() -> OptionsBuilder {
        OptionsBuilder::default()
    }

    pub fn tool_strict_enabled(&self) -> bool {
        self.tool_strict.unwrap_or(true)
    }

    pub fn merge(&mut self, other: Options) {
        if let Some(prompt_model) = other.prompt_model {
            self.prompt_model = Some(prompt_model);
        }
        if let Some(max_tokens) = other.max_tokens {
            self.max_tokens = Some(max_tokens);
        }
        if let Some(temperature) = other.temperature {
            self.temperature = Some(temperature);
        }
        if let Some(top_p) = other.top_p {
            self.top_p = Some(top_p);
        }
        if let Some(stop_sequences) = other.stop_sequences {
            self.stop_sequences = Some(stop_sequences);
        }
        if let Some(tool_strict) = other.tool_strict {
            self.tool_strict = Some(tool_strict);
        }
        if let Some(additional_model_request_fields) = other.additional_model_request_fields {
            self.additional_model_request_fields = Some(additional_model_request_fields);
        }
        if let Some(additional_model_response_field_paths) =
            other.additional_model_response_field_paths
        {
            self.additional_model_response_field_paths =
                Some(additional_model_response_field_paths);
        }
    }
}

impl AwsBedrock {
    pub fn builder() -> AwsBedrockBuilder {
        AwsBedrockBuilder::default()
    }

    /// Retrieve a reference to the default options.
    pub fn options(&self) -> &Options {
        &self.default_options
    }

    /// Retrieve a mutable reference to the default options.
    pub fn options_mut(&mut self) -> &mut Options {
        &mut self.default_options
    }

    fn prompt_model(&self) -> Result<&str, LanguageModelError> {
        self.default_options
            .prompt_model
            .as_deref()
            .ok_or_else(|| LanguageModelError::PermanentError("Model not set".into()))
    }

    async fn report_usage(&self, model: &str, usage: &Usage) -> Result<(), LanguageModelError> {
        #[cfg(not(feature = "metrics"))]
        let _ = model;

        if let Some(callback) = &self.on_usage {
            callback(usage).await?;
        }

        #[cfg(feature = "metrics")]
        {
            swiftide_core::metrics::emit_usage(
                model,
                usage.prompt_tokens.into(),
                usage.completion_tokens.into(),
                usage.total_tokens.into(),
                self.metric_metadata.as_ref(),
            );
        }

        Ok(())
    }

    #[allow(unused_variables)]
    async fn track_completion<R, S>(
        &self,
        model: &str,
        usage: Option<&Usage>,
        request: Option<&R>,
        response: Option<&S>,
    ) -> Result<(), LanguageModelError>
    where
        R: Serialize + ?Sized,
        S: Serialize + ?Sized,
    {
        if let Some(usage) = usage {
            self.report_usage(model, usage).await?;
        }

        #[cfg(feature = "langfuse")]
        tracing::debug!(
            langfuse.model = model,
            langfuse.input = request.and_then(langfuse_json_redacted).unwrap_or_default(),
            langfuse.output = response.and_then(langfuse_json).unwrap_or_default(),
            langfuse.usage = usage.and_then(langfuse_json).unwrap_or_default(),
        );

        Ok(())
    }
}

impl AwsBedrockBuilder {
    #[allow(clippy::unused_self)]
    fn default_config(&self) -> aws_config::SdkConfig {
        tokio::task::block_in_place(|| Handle::current().block_on(aws_config::load_from_env()))
    }

    fn default_client(&self) -> Arc<Client> {
        Arc::new(Client::new(&self.default_config()))
    }

    /// Sets the Bedrock runtime client.
    pub fn client(&mut self, client: Client) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }

    /// Sets the default prompt model for Converse requests.
    pub fn default_prompt_model(&mut self, model: impl Into<String>) -> &mut Self {
        if let Some(options) = self.default_options.as_mut() {
            options.prompt_model = Some(model.into());
        } else {
            self.default_options = Some(Options {
                prompt_model: Some(model.into()),
                ..Default::default()
            });
        }

        self
    }

    /// Sets default options for requests.
    ///
    /// Merges with existing options if already set.
    pub fn default_options(&mut self, options: impl Into<Options>) -> &mut Self {
        let options = options.into();
        if let Some(existing_options) = self.default_options.as_mut() {
            existing_options.merge(options);
        } else {
            self.default_options = Some(options);
        }

        self
    }

    /// Adds a callback function that will be called when usage information is available.
    pub fn on_usage<F>(&mut self, func: F) -> &mut Self
    where
        F: Fn(&Usage) -> anyhow::Result<()> + Send + Sync + 'static,
    {
        let func = Arc::new(func);
        self.on_usage = Some(Some(Arc::new(move |usage: &Usage| {
            let func = func.clone();
            Box::pin(async move { func(usage) })
        })));

        self
    }

    /// Adds an asynchronous callback function that will be called when usage information is
    /// available.
    pub fn on_usage_async<F>(&mut self, func: F) -> &mut Self
    where
        F: for<'a> Fn(
                &'a Usage,
            )
                -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>>
            + Send
            + Sync
            + 'static,
    {
        let func = Arc::new(func);
        self.on_usage = Some(Some(Arc::new(move |usage: &Usage| {
            let func = func.clone();
            Box::pin(async move { func(usage).await })
        })));

        self
    }

    #[cfg(test)]
    #[allow(private_bounds)]
    pub fn test_client(&mut self, client: impl BedrockConverse + 'static) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }
}

#[cfg_attr(test, automock)]
#[async_trait]
#[allow(clippy::too_many_arguments)]
trait BedrockConverse: std::fmt::Debug + Send + Sync {
    async fn converse(
        &self,
        model_id: &str,
        messages: Vec<Message>,
        system: Option<Vec<SystemContentBlock>>,
        inference_config: Option<InferenceConfiguration>,
        tool_config: Option<ToolConfiguration>,
        output_config: Option<OutputConfig>,
        additional_model_request_fields: Option<Document>,
        additional_model_response_field_paths: Option<Vec<String>>,
    ) -> Result<ConverseOutput, LanguageModelError>;

    async fn converse_stream(
        &self,
        model_id: &str,
        messages: Vec<Message>,
        system: Option<Vec<SystemContentBlock>>,
        inference_config: Option<InferenceConfiguration>,
        tool_config: Option<ToolConfiguration>,
        additional_model_request_fields: Option<Document>,
        additional_model_response_field_paths: Option<Vec<String>>,
    ) -> Result<BedrockConverseStreamOutput, LanguageModelError>;
}

#[async_trait]
#[allow(clippy::too_many_arguments)]
impl BedrockConverse for Client {
    async fn converse(
        &self,
        model_id: &str,
        messages: Vec<Message>,
        system: Option<Vec<SystemContentBlock>>,
        inference_config: Option<InferenceConfiguration>,
        tool_config: Option<ToolConfiguration>,
        output_config: Option<OutputConfig>,
        additional_model_request_fields: Option<Document>,
        additional_model_response_field_paths: Option<Vec<String>>,
    ) -> Result<ConverseOutput, LanguageModelError> {
        let mut request = self
            .converse()
            .model_id(model_id)
            .set_messages(Some(messages))
            .set_system(system)
            .set_tool_config(tool_config)
            .set_output_config(output_config)
            .set_additional_model_request_fields(additional_model_request_fields)
            .set_additional_model_response_field_paths(additional_model_response_field_paths);

        if let Some(inference_config) = inference_config {
            request = request.inference_config(inference_config);
        }

        request
            .send()
            .await
            .map_err(converse_error_to_language_model_error)
    }

    async fn converse_stream(
        &self,
        model_id: &str,
        messages: Vec<Message>,
        system: Option<Vec<SystemContentBlock>>,
        inference_config: Option<InferenceConfiguration>,
        tool_config: Option<ToolConfiguration>,
        additional_model_request_fields: Option<Document>,
        additional_model_response_field_paths: Option<Vec<String>>,
    ) -> Result<BedrockConverseStreamOutput, LanguageModelError> {
        let mut request = self
            .converse_stream()
            .model_id(model_id)
            .set_messages(Some(messages))
            .set_system(system)
            .set_tool_config(tool_config)
            .set_additional_model_request_fields(additional_model_request_fields)
            .set_additional_model_response_field_paths(additional_model_response_field_paths);

        if let Some(inference_config) = inference_config {
            request = request.inference_config(inference_config);
        }

        request
            .send()
            .await
            .map_err(converse_stream_error_to_language_model_error)
    }
}

fn converse_error_to_language_model_error<R>(
    error: SdkError<ConverseError, R>,
) -> LanguageModelError
where
    R: std::fmt::Debug + Send + Sync + 'static,
{
    sdk_error_to_language_model_error(error, |service_error| {
        matches!(
            service_error,
            ConverseError::ThrottlingException(_)
                | ConverseError::ServiceUnavailableException(_)
                | ConverseError::ModelNotReadyException(_)
                | ConverseError::ModelTimeoutException(_)
                | ConverseError::InternalServerException(_)
        )
    })
}

fn converse_stream_error_to_language_model_error<R>(
    error: SdkError<ConverseStreamError, R>,
) -> LanguageModelError
where
    R: std::fmt::Debug + Send + Sync + 'static,
{
    sdk_error_to_language_model_error(error, |service_error| {
        matches!(
            service_error,
            ConverseStreamError::ThrottlingException(_)
                | ConverseStreamError::ServiceUnavailableException(_)
                | ConverseStreamError::ModelNotReadyException(_)
                | ConverseStreamError::ModelTimeoutException(_)
                | ConverseStreamError::InternalServerException(_)
                | ConverseStreamError::ModelStreamErrorException(_)
        )
    })
}

fn converse_stream_output_error_to_language_model_error<R>(
    error: SdkError<ConverseStreamOutputError, R>,
) -> LanguageModelError
where
    R: std::fmt::Debug + Send + Sync + 'static,
{
    sdk_error_to_language_model_error(error, |service_error| {
        matches!(
            service_error,
            ConverseStreamOutputError::ThrottlingException(_)
                | ConverseStreamOutputError::ServiceUnavailableException(_)
                | ConverseStreamOutputError::InternalServerException(_)
                | ConverseStreamOutputError::ModelStreamErrorException(_)
        )
    })
}

fn sdk_error_to_language_model_error<E, R>(
    error: SdkError<E, R>,
    is_transient_service_error: impl Fn(&E) -> bool,
) -> LanguageModelError
where
    E: std::error::Error + Send + Sync + 'static,
    R: std::fmt::Debug + Send + Sync + 'static,
{
    let is_transient = match &error {
        SdkError::TimeoutError(_) | SdkError::DispatchFailure(_) | SdkError::ResponseError(_) => {
            true
        }
        SdkError::ServiceError(service_error) => is_transient_service_error(service_error.err()),
        _ => false,
    };
    let detailed_error = match error {
        SdkError::ServiceError(service_error) => anyhow::Error::new(service_error.into_err()),
        error => anyhow::Error::msg(error_chain_message(&error)),
    };

    if is_transient {
        LanguageModelError::transient(detailed_error)
    } else {
        LanguageModelError::permanent(detailed_error)
    }
}

fn error_chain_message(error: &(dyn std::error::Error + 'static)) -> String {
    std::iter::successors(Some(error), |err| err.source())
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ")
}

fn inference_config_from_options(options: &Options) -> Option<InferenceConfiguration> {
    let mut builder = InferenceConfiguration::builder();
    let mut has_any_value = false;

    if let Some(max_tokens) = options.max_tokens {
        builder = builder.max_tokens(max_tokens);
        has_any_value = true;
    }

    if let Some(temperature) = options.temperature {
        builder = builder.temperature(temperature);
        has_any_value = true;
    }

    if let Some(top_p) = options.top_p {
        builder = builder.top_p(top_p);
        has_any_value = true;
    }

    if let Some(stop_sequences) = &options.stop_sequences {
        builder = builder.set_stop_sequences(Some(stop_sequences.clone()));
        has_any_value = true;
    }

    has_any_value.then(|| builder.build())
}

fn usage_from_bedrock(usage: &TokenUsage) -> Usage {
    let cached_tokens = usage
        .cache_read_input_tokens()
        .and_then(i32_to_u32)
        .or_else(|| usage.cache_write_input_tokens().and_then(i32_to_u32));

    let details = cached_tokens.map(|cached_tokens| UsageDetails {
        input_tokens_details: Some(InputTokenDetails {
            cached_tokens: Some(cached_tokens),
        }),
        ..Default::default()
    });

    Usage {
        prompt_tokens: i32_to_u32(usage.input_tokens()).unwrap_or_default(),
        completion_tokens: i32_to_u32(usage.output_tokens()).unwrap_or_default(),
        total_tokens: i32_to_u32(usage.total_tokens()).unwrap_or_default(),
        details,
    }
}

fn context_length_exceeded_if_empty(
    has_message: bool,
    has_tool_calls: bool,
    has_reasoning: bool,
    stop_reason: Option<&StopReason>,
) -> Option<LanguageModelError> {
    if has_message
        || has_tool_calls
        || has_reasoning
        || !matches!(stop_reason, Some(StopReason::ModelContextWindowExceeded))
    {
        return None;
    }

    Some(LanguageModelError::context_length_exceeded(
        "Model context window exceeded",
    ))
}

fn i32_to_u32(value: i32) -> Option<u32> {
    u32::try_from(value).ok()
}

#[cfg(feature = "langfuse")]
fn langfuse_json<T: Serialize + ?Sized>(value: &T) -> Option<String> {
    serde_json::to_string_pretty(value).ok()
}

#[cfg(feature = "langfuse")]
fn langfuse_json_redacted<T: Serialize + ?Sized>(value: &T) -> Option<String> {
    let mut value = serde_json::to_value(value).ok()?;
    redact_sensitive_payloads(&mut value);
    serde_json::to_string_pretty(&value).ok()
}

#[cfg(feature = "langfuse")]
fn redact_sensitive_payloads(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for field in map.values_mut() {
                redact_sensitive_payloads(field);
            }
        }
        serde_json::Value::Array(items) => {
            if items.iter().all(|item| item.as_u64().is_some()) && items.len() > 64 {
                *value = serde_json::Value::String(format!("[{} bytes redacted]", items.len()));
            } else {
                for item in items {
                    redact_sensitive_payloads(item);
                }
            }
        }
        serde_json::Value::String(text) => {
            if let Some(truncated) = truncate_data_url(text) {
                *text = truncated;
            }
        }
        _ => {}
    }
}

#[cfg(feature = "langfuse")]
fn truncate_data_url(url: &str) -> Option<String> {
    const MAX_DATA_PREVIEW: usize = 32;

    if !url.starts_with("data:") {
        return None;
    }

    let (prefix, data) = url.split_once(',')?;
    if data.len() <= MAX_DATA_PREVIEW {
        return None;
    }

    let preview = &data[..MAX_DATA_PREVIEW];
    let truncated = data.len() - MAX_DATA_PREVIEW;

    Some(format!(
        "{prefix},{preview}...[truncated {truncated} chars]"
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    };

    use aws_sdk_bedrockruntime::{
        error::{ConnectorError, SdkError},
        operation::{converse::ConverseError, converse_stream::ConverseStreamError},
        types::{
            StopReason, TokenUsage,
            error::{
                ConverseStreamOutputError, InternalServerException, ModelNotReadyException,
                ModelStreamErrorException, ServiceUnavailableException, ThrottlingException,
                ValidationException,
            },
        },
    };
    use swiftide_core::chat_completion::errors::LanguageModelError;

    use super::*;

    fn usage(total_tokens: u32) -> Usage {
        Usage {
            prompt_tokens: total_tokens / 2,
            completion_tokens: total_tokens - (total_tokens / 2),
            total_tokens,
            details: None,
        }
    }

    #[test]
    fn test_options_builder_and_merge_only_overrides_present_fields() {
        let mut base = Options::builder()
            .prompt_model("model-a")
            .max_tokens(128)
            .temperature(0.1)
            .top_p(0.8)
            .stop_sequences(vec!["STOP_A".to_string()])
            .tool_strict(false)
            .build()
            .unwrap();

        let mut request_fields = std::collections::HashMap::new();
        request_fields.insert("thinking".to_string(), Document::Bool(true));

        let other = Options {
            prompt_model: Some("model-b".to_string()),
            max_tokens: None,
            temperature: Some(0.6),
            top_p: None,
            stop_sequences: Some(vec!["STOP_B".to_string()]),
            tool_strict: Some(true),
            additional_model_request_fields: Some(Document::Object(request_fields)),
            additional_model_response_field_paths: Some(vec!["/thinking".to_string()]),
        };

        base.merge(other);

        assert_eq!(base.prompt_model.as_deref(), Some("model-b"));
        assert_eq!(base.max_tokens, Some(128));
        assert_eq!(base.temperature, Some(0.6));
        assert_eq!(base.top_p, Some(0.8));
        assert_eq!(
            base.stop_sequences.as_deref(),
            Some(&["STOP_B".to_string()][..])
        );
        assert_eq!(base.tool_strict, Some(true));
        assert!(base.additional_model_request_fields.is_some());
        assert_eq!(
            base.additional_model_response_field_paths.as_deref(),
            Some(&["/thinking".to_string()][..])
        );
    }

    #[test]
    fn test_tool_strict_enabled_defaults_to_true() {
        assert!(Options::default().tool_strict_enabled());
        assert!(
            !Options {
                tool_strict: Some(false),
                ..Default::default()
            }
            .tool_strict_enabled()
        );
    }

    #[test]
    fn test_builder_default_options_and_prompt_model_merge_branches() {
        let mut builder = AwsBedrock::builder();
        builder.test_client(MockBedrockConverse::new());

        builder.default_prompt_model("model-initial");
        builder.default_prompt_model("model-final");

        builder.default_options(Options {
            max_tokens: Some(64),
            ..Default::default()
        });
        builder.default_options(Options {
            temperature: Some(0.7),
            ..Default::default()
        });

        let mut client = builder.build().unwrap();
        assert_eq!(
            client.options().prompt_model.as_deref(),
            Some("model-final")
        );
        assert_eq!(client.options().max_tokens, Some(64));
        assert_eq!(client.options().temperature, Some(0.7));

        client.options_mut().top_p = Some(0.9);
        assert_eq!(client.options().top_p, Some(0.9));
        assert!(format!("{client:?}").contains("AwsBedrock"));
    }

    #[test_log::test(tokio::test)]
    async fn test_track_completion_invokes_sync_usage_callback() {
        let observed = Arc::new(AtomicU32::new(0));
        let observed_for_callback = observed.clone();

        let mut builder = AwsBedrock::builder();
        builder
            .test_client(MockBedrockConverse::new())
            .default_prompt_model("model-a")
            .on_usage(move |usage| {
                observed_for_callback.store(usage.total_tokens, Ordering::Relaxed);
                Ok(())
            });

        let bedrock = builder.build().unwrap();
        let req = serde_json::json!({"request": "value"});
        let resp = serde_json::json!({"response": "value"});
        let usage = usage(42);

        bedrock
            .track_completion("model-a", Some(&usage), Some(&req), Some(&resp))
            .await
            .unwrap();

        assert_eq!(observed.load(Ordering::Relaxed), 42);
    }

    #[test_log::test(tokio::test)]
    async fn test_track_completion_invokes_async_usage_callback() {
        let observed = Arc::new(AtomicU32::new(0));
        let observed_for_callback = observed.clone();

        let mut builder = AwsBedrock::builder();
        builder
            .test_client(MockBedrockConverse::new())
            .default_prompt_model("model-a")
            .on_usage_async(move |usage| {
                let observed_for_callback = observed_for_callback.clone();
                Box::pin(async move {
                    observed_for_callback.store(usage.total_tokens, Ordering::Relaxed);
                    Ok(())
                })
            });

        let bedrock = builder.build().unwrap();
        let usage = usage(99);

        bedrock
            .track_completion(
                "model-a",
                Some(&usage),
                None::<&serde_json::Value>,
                None::<&serde_json::Value>,
            )
            .await
            .unwrap();

        assert_eq!(observed.load(Ordering::Relaxed), 99);
    }

    #[test]
    fn test_inference_config_from_options_builds_only_when_values_are_set() {
        assert!(inference_config_from_options(&Options::default()).is_none());

        let options = Options {
            max_tokens: Some(256),
            temperature: Some(0.2),
            top_p: Some(0.9),
            stop_sequences: Some(vec!["DONE".to_string()]),
            ..Default::default()
        };

        let config = inference_config_from_options(&options).expect("inference config");
        assert_eq!(config.max_tokens(), Some(256));
        assert_eq!(config.temperature(), Some(0.2));
        assert_eq!(config.top_p(), Some(0.9));
        assert_eq!(config.stop_sequences(), ["DONE"]);
    }

    #[test]
    fn test_usage_from_bedrock_prefers_cache_read_and_falls_back_to_cache_write() {
        let read_usage = TokenUsage::builder()
            .input_tokens(10)
            .output_tokens(5)
            .total_tokens(15)
            .cache_read_input_tokens(3)
            .cache_write_input_tokens(9)
            .build()
            .unwrap();
        let mapped_read = usage_from_bedrock(&read_usage);
        assert_eq!(
            mapped_read
                .details
                .as_ref()
                .and_then(|details| details.input_tokens_details.as_ref())
                .and_then(|details| details.cached_tokens),
            Some(3)
        );

        let write_usage = TokenUsage::builder()
            .input_tokens(10)
            .output_tokens(5)
            .total_tokens(15)
            .cache_write_input_tokens(7)
            .build()
            .unwrap();
        let mapped_write = usage_from_bedrock(&write_usage);
        assert_eq!(
            mapped_write
                .details
                .as_ref()
                .and_then(|details| details.input_tokens_details.as_ref())
                .and_then(|details| details.cached_tokens),
            Some(7)
        );
    }

    #[test]
    fn test_usage_from_bedrock_defaults_negative_counts_to_zero() {
        let usage = TokenUsage::builder()
            .input_tokens(-1)
            .output_tokens(-2)
            .total_tokens(-3)
            .build()
            .unwrap();
        let mapped = usage_from_bedrock(&usage);

        assert_eq!(mapped.prompt_tokens, 0);
        assert_eq!(mapped.completion_tokens, 0);
        assert_eq!(mapped.total_tokens, 0);
        assert_eq!(i32_to_u32(-1), None);
        assert_eq!(i32_to_u32(12), Some(12));
    }

    #[test]
    fn test_context_length_exceeded_only_when_empty_and_context_limit_hit() {
        assert!(
            context_length_exceeded_if_empty(
                false,
                false,
                false,
                Some(&StopReason::ModelContextWindowExceeded)
            )
            .is_some()
        );
        assert!(context_length_exceeded_if_empty(true, false, false, None).is_none());
        assert!(context_length_exceeded_if_empty(false, true, false, None).is_none());
        assert!(context_length_exceeded_if_empty(false, false, true, None).is_none());
        assert!(
            context_length_exceeded_if_empty(false, false, false, Some(&StopReason::EndTurn))
                .is_none()
        );
    }

    #[test]
    fn test_sdk_error_mapping_classifies_transient_transport_failures() {
        let timeout = sdk_error_to_language_model_error::<ConverseError, ()>(
            SdkError::timeout_error("timeout"),
            |_| false,
        );
        assert!(matches!(timeout, LanguageModelError::TransientError(_)));

        let dispatch = sdk_error_to_language_model_error::<ConverseError, ()>(
            SdkError::dispatch_failure(ConnectorError::other("dispatch".into(), None)),
            |_| false,
        );
        assert!(matches!(dispatch, LanguageModelError::TransientError(_)));

        let response = sdk_error_to_language_model_error::<ConverseError, ()>(
            SdkError::response_error("response", ()),
            |_| false,
        );
        assert!(matches!(response, LanguageModelError::TransientError(_)));

        let construction = sdk_error_to_language_model_error::<ConverseError, ()>(
            SdkError::construction_failure("construction"),
            |_| false,
        );
        assert!(matches!(
            construction,
            LanguageModelError::PermanentError(_)
        ));
    }

    #[test]
    fn test_converse_error_mapping_distinguishes_transient_and_permanent_service_errors() {
        let throttled = converse_error_to_language_model_error::<()>(SdkError::service_error(
            ConverseError::ThrottlingException(ThrottlingException::builder().build()),
            (),
        ));
        assert!(matches!(throttled, LanguageModelError::TransientError(_)));

        let validation = converse_error_to_language_model_error::<()>(SdkError::service_error(
            ConverseError::ValidationException(ValidationException::builder().build()),
            (),
        ));
        assert!(matches!(validation, LanguageModelError::PermanentError(_)));
    }

    #[test]
    fn test_converse_stream_error_mapping_distinguishes_transient_and_permanent_service_errors() {
        let unavailable =
            converse_stream_error_to_language_model_error::<()>(SdkError::service_error(
                ConverseStreamError::ServiceUnavailableException(
                    ServiceUnavailableException::builder().build(),
                ),
                (),
            ));
        assert!(matches!(unavailable, LanguageModelError::TransientError(_)));

        let validation =
            converse_stream_error_to_language_model_error::<()>(SdkError::service_error(
                ConverseStreamError::ValidationException(ValidationException::builder().build()),
                (),
            ));
        assert!(matches!(validation, LanguageModelError::PermanentError(_)));
    }

    #[test]
    fn test_converse_stream_output_error_mapping_distinguishes_transient_and_permanent_service_errors()
     {
        let transient =
            converse_stream_output_error_to_language_model_error::<()>(SdkError::service_error(
                ConverseStreamOutputError::ModelStreamErrorException(
                    ModelStreamErrorException::builder().build(),
                ),
                (),
            ));
        assert!(matches!(transient, LanguageModelError::TransientError(_)));

        let permanent =
            converse_stream_output_error_to_language_model_error::<()>(SdkError::service_error(
                ConverseStreamOutputError::ValidationException(
                    ValidationException::builder().build(),
                ),
                (),
            ));
        assert!(matches!(permanent, LanguageModelError::PermanentError(_)));
    }

    #[test]
    fn test_error_chain_message_collects_nested_sources() {
        let source = std::io::Error::other("inner");
        let outer = std::io::Error::other(source);
        let chain = error_chain_message(&outer);

        assert!(chain.contains("inner"));
    }

    #[test]
    fn test_converse_error_mapping_model_not_ready_and_stream_internal_server_are_transient() {
        let model_not_ready =
            converse_error_to_language_model_error::<()>(SdkError::service_error(
                ConverseError::ModelNotReadyException(ModelNotReadyException::builder().build()),
                (),
            ));
        assert!(matches!(
            model_not_ready,
            LanguageModelError::TransientError(_)
        ));

        let stream_internal =
            converse_stream_output_error_to_language_model_error::<()>(SdkError::service_error(
                ConverseStreamOutputError::InternalServerException(
                    InternalServerException::builder().build(),
                ),
                (),
            ));
        assert!(matches!(
            stream_internal,
            LanguageModelError::TransientError(_)
        ));
    }
}
