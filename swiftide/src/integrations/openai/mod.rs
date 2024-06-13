use std::sync::Arc;

use derive_builder::Builder;

mod embed;
mod simple_prompt;

#[derive(Debug, Builder, Clone)]
pub struct OpenAI {
    #[builder(default = "Arc::new(async_openai::Client::new())", setter(custom))]
    client: Arc<async_openai::Client<async_openai::config::OpenAIConfig>>,
    #[builder(default)]
    default_options: Options,
}

#[derive(Debug, Default, Clone, Builder)]
#[builder(setter(into, strip_option))]
pub struct Options {
    #[builder(default)]
    pub embed_model: Option<String>,
    #[builder(default)]
    pub prompt_model: Option<String>,
}

impl Options {
    pub fn builder() -> OptionsBuilder {
        OptionsBuilder::default()
    }
}

impl OpenAI {
    pub fn builder() -> OpenAIBuilder {
        OpenAIBuilder::default()
    }
}

impl OpenAIBuilder {
    pub fn client(
        &mut self,
        client: async_openai::Client<async_openai::config::OpenAIConfig>,
    ) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }

    pub fn default_embed_model(&mut self, model: impl Into<String>) -> &mut Self {
        if let Some(options) = self.default_options.as_mut() {
            options.embed_model = Some(model.into());
        } else {
            self.default_options = Some(Options {
                embed_model: Some(model.into()),
                ..Default::default()
            });
        }
        self
    }

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
}
