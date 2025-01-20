use std::sync::Arc;

use config::DashscopeConfig;
use derive_builder::Builder;

mod config;
mod embed;
mod simple_prompt;

#[derive(Debug, Builder, Clone)]
#[builder(setter(into, strip_option))]
pub struct Dashscope {
    #[builder(default = "default_client()", setter(custom))]
    client: Arc<async_openai::Client<DashscopeConfig>>,
    /// Default options for prompt models.
    #[builder(default)]
    default_options: Options,
}

impl Default for Dashscope {
    fn default() -> Self {
        Self {
            client: default_client(),
            default_options: Options::default(),
        }
    }
}

fn default_client() -> Arc<async_openai::Client<DashscopeConfig>> {
    async_openai::Client::with_config(DashscopeConfig::default()).into()
}

#[derive(Debug, Default, Clone, Builder)]
#[builder(setter(into, strip_option))]
pub struct Options {
    /// The default prompt model to use, if specified.
    #[builder(default)]
    pub prompt_model: Option<String>,
    #[builder(default)]
    pub embed_model: Option<String>,
    #[builder(default)]
    pub dimensions: u16,
}

impl Options {
    /// Creates a new `OptionsBuilder` for constructing `Options` instances.
    pub fn builder() -> OptionsBuilder {
        OptionsBuilder::default()
    }
}

impl Dashscope {
    /// Creates a new `DashscopeBuilder` for constructing `Dashscope` instances.
    pub fn builder() -> DashscopeBuilder {
        DashscopeBuilder::default()
    }

    /// Sets a default prompt model to use when prompting
    pub fn with_default_prompt_model(&mut self, model: impl Into<String>) -> &mut Self {
        self.default_options = Options {
            prompt_model: Some(model.into()),
            ..Default::default()
        };
        self
    }

    pub fn with_default_embed_model(&mut self, model: impl Into<String>) -> &mut Self {
        self.default_options = Options {
            embed_model: Some(model.into()),
            ..Default::default()
        };
        self
    }
}

impl DashscopeBuilder {
    /// Sets the `Dashscope` client for the `Dashscope` instance.
    ///
    /// # Parameters
    /// - `client`: The `Dashscope` client to set.
    ///
    /// # Returns
    /// A mutable reference to the `DashscopeBuilder`.
    pub fn client(&mut self, client: async_openai::Client<DashscopeConfig>) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }

    /// Sets the default prompt model for the `Dashscope` instance.
    ///
    /// # Parameters
    /// - `model`: The prompt model to set.
    ///
    /// # Returns
    /// A mutable reference to the `DashscopeBuilder`.
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

    /// Sets the default dimensions for the `Dashscope` instance.
    ///
    /// # Parameters
    /// - `dimensions`: The dimensions to set.
    ///
    /// # Returns
    /// A mutable reference to the `DashscopeBuilder`.
    pub fn default_dimensions(&mut self, dimensions: u16) -> &mut Self {
        if let Some(options) = self.default_options.as_mut() {
            options.dimensions = dimensions;
        } else {
            self.default_options = Some(Options {
                dimensions,
                ..Default::default()
            });
        }
        self
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_default_prompt_model() {
        let openai = Dashscope::builder()
            .default_prompt_model("qwen-long")
            .build()
            .unwrap();
        assert_eq!(
            openai.default_options.prompt_model,
            Some("qwen-long".to_string())
        );

        let openai = Dashscope::builder()
            .default_prompt_model("qwen-turbo")
            .build()
            .unwrap();
        assert_eq!(
            openai.default_options.prompt_model,
            Some("qwen-turbo".to_string())
        );
    }
}
