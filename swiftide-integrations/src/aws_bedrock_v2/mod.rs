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
use swiftide_core::chat_completion::{
    InputTokenDetails, Usage, UsageDetails, errors::LanguageModelError,
};
use tokio::runtime::Handle;

#[cfg(test)]
use mockall::automock;

mod chat_completion;
mod simple_prompt;
mod structured_prompt;

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
}

impl AwsBedrockBuilder {
    #[allow(clippy::unused_self)]
    fn default_config(&self) -> aws_config::SdkConfig {
        tokio::task::block_in_place(|| {
            Handle::current().block_on(async {
                aws_config::defaults(aws_config::BehaviorVersion::latest())
                    .load()
                    .await
            })
        })
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
