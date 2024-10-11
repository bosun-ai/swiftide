use anyhow::Result;
use std::{
    fmt::{Debug, Formatter},
    sync::Arc,
};
use swiftide_core::{prompt::Prompt, SimplePrompt};

use async_trait::async_trait;
use derive_builder::Builder;
#[allow(unused_imports)]
pub use mistralrs::{IsqType, PagedAttentionMetaBuilder, TextModelBuilder};
use mistralrs::{RequestBuilder, TextMessageRole};

/// Prompt popular hugging face text models provided by [`mistralrs`].
///
/// You can either provide a model name, or use the mistral.
/// See [the mistral.rs github page](https://github.com/EricLBuehler/mistral.rs/) for more
/// information and a list of supported models.
///
/// When building the model, when using the default implementation, it will block on the current
/// runtime to build the model. Ensure that you only do this once.
///
///
/// # Example
///
/// ```ignore
/// let model = MistralTextModel::builder()
///     .model_name("microsoft/Phi-3.5-mini-instruct")
///     .build()?;
///
/// // ... later in the pipeline
/// pipeline.then(MetadataQACode::new(model))
/// ```
#[derive(Builder, Clone)]
// #[builder(pattern = "owned")]
#[builder(setter(into), build_fn(error = "anyhow::Error"))]
pub struct MistralTextModel {
    /// Internal model used. Can be overwritten via the builder.
    ///
    /// See [`::mistralrs::TextModelBuilder`].
    #[builder(default = "self.default_from_model_name()?")]
    model: Arc<::mistralrs::v0_4_api::Model>,

    /// Optional model name to build a default model
    #[builder(default, setter(strip_option))]
    #[allow(dead_code)]
    model_name: Option<String>,
}

impl MistralTextModel {
    pub fn builder() -> MistralTextModelBuilder {
        MistralTextModelBuilder::default()
    }
}

impl MistralTextModelBuilder {
    fn default_from_model_name(&self) -> Result<Arc<::mistralrs::v0_4_api::Model>> {
        tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current()
                .block_on(async { self.default_from_model_name_async().await })
        })
    }

    async fn default_from_model_name_async(&self) -> Result<Arc<::mistralrs::v0_4_api::Model>> {
        let model_name = self
            .model_name
            .clone()
            .flatten()
            .ok_or(anyhow::anyhow!("Missing model name"))?;

        tracing::warn!("Setting up model {model_name}");

        let model = Arc::new(
            TextModelBuilder::new(&model_name)
                .with_paged_attn(|| PagedAttentionMetaBuilder::default().build())?
                .build()
                .await?,
        );

        tracing::warn!("Set up model {model_name}");

        Ok(model)
    }
}

impl Debug for MistralTextModel {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MistralPrompt").finish()
    }
}

#[async_trait]
impl SimplePrompt for MistralTextModel {
    #[tracing::instrument(skip_all)]
    async fn prompt(&self, prompt: Prompt) -> Result<String> {
        let request =
            RequestBuilder::new().add_message(TextMessageRole::User, prompt.render().await?);

        tracing::info!("Sending request to mistral prompt");

        let mut response = self.model.send_chat_request(request).await?;
        response.choices[0]
            .message
            .content
            .take()
            .ok_or_else(|| anyhow::anyhow!("No content in response"))
    }
}
