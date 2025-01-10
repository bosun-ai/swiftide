use std::{fmt::Display, sync::Arc};

use config::QwenConfig;
use derive_builder::Builder;

mod config;
mod embed;
mod simple_prompt;

#[derive(Debug, Default, Clone, PartialEq)]
pub enum QwenModel {
    #[default]
    Max,
    Plus,
    Turbo,
    Long,
}

impl Display for QwenModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QwenModel::Max => write!(f, "qwen-max"),
            QwenModel::Plus => write!(f, "qwen-plus"),
            QwenModel::Turbo => write!(f, "qwen-turbo"),
            QwenModel::Long => write!(f, "qwen-long"),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub enum QwenEmbedding {
    #[default]
    TextEmbeddingV1,
    TextEmbeddingV2,
    TextEmbeddingV3,
    TextEmbeddingAsyncV1,
    TextEmbeddingAsyncV2,
}

impl Display for QwenEmbedding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QwenEmbedding::TextEmbeddingV1 => write!(f, "text-embedding-v1"),
            QwenEmbedding::TextEmbeddingV2 => write!(f, "text-embedding-v2"),
            QwenEmbedding::TextEmbeddingV3 => write!(f, "text-embedding-v3"),
            QwenEmbedding::TextEmbeddingAsyncV1 => write!(
                f,
                "text-embedding-async-v1

"
            ),
            QwenEmbedding::TextEmbeddingAsyncV2 => write!(
                f,
                "text-embedding-async-v2

"
            ),
        }
    }
}

impl From<&String> for QwenEmbedding {
    fn from(value: &String) -> Self {
        match value.as_str() {
            "text-embedding-v1" => QwenEmbedding::TextEmbeddingV1,
            "text-embedding-v2" => QwenEmbedding::TextEmbeddingV2,
            "text-embedding-v3" => QwenEmbedding::TextEmbeddingV3,
            "text-embedding-async-v1" => QwenEmbedding::TextEmbeddingAsyncV1,
            "text-embedding-async-v2" => QwenEmbedding::TextEmbeddingAsyncV2,
            _ => panic!("Invalid embedding model"),
        }
    }
}

#[derive(Debug, Builder, Clone)]
#[builder(setter(into, strip_option))]
pub struct Qwen {
    #[builder(default = "default_client()", setter(custom))]
    client: Arc<async_openai::Client<QwenConfig>>,
    /// Default options for prompt models.
    #[builder(default)]
    default_options: Options,
}

impl Default for Qwen {
    fn default() -> Self {
        Self {
            client: default_client(),
            default_options: Options::default(),
        }
    }
}

fn default_client() -> Arc<async_openai::Client<QwenConfig>> {
    async_openai::Client::with_config(QwenConfig::default()).into()
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

impl Qwen {
    /// Creates a new `QwenBuilder` for constructing `Qwen` instances.
    pub fn builder() -> QwenBuilder {
        QwenBuilder::default()
    }

    /// Sets a default prompt model to use when prompting
    pub fn with_default_prompt_model(&mut self, model: &QwenModel) -> &mut Self {
        self.default_options = Options {
            prompt_model: Some(model.to_string()),
            ..Default::default()
        };
        self
    }

    pub fn with_default_embed_model(&mut self, model: &QwenEmbedding) -> &mut Self {
        self.default_options = Options {
            embed_model: Some(model.to_string()),
            ..Default::default()
        };
        self
    }
}

impl QwenBuilder {
    /// Sets the `Qwen` client for the `Qwen` instance.
    ///
    /// # Parameters
    /// - `client`: The `Qwen` client to set.
    ///
    /// # Returns
    /// A mutable reference to the `QwenBuilder`.
    pub fn client(&mut self, client: async_openai::Client<QwenConfig>) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }

    /// Sets the default prompt model for the `Qwen` instance.
    ///
    /// # Parameters
    /// - `model`: The prompt model to set.
    ///
    /// # Returns
    /// A mutable reference to the `QwenBuilder`.
    pub fn default_prompt_model(&mut self, model: &QwenModel) -> &mut Self {
        if let Some(options) = self.default_options.as_mut() {
            options.prompt_model = Some(model.to_string());
        } else {
            self.default_options = Some(Options {
                prompt_model: Some(model.to_string()),
                ..Default::default()
            });
        }
        self
    }

    pub fn default_embed_model(&mut self, model: &QwenEmbedding) -> &mut Self {
        if let Some(options) = self.default_options.as_mut() {
            options.embed_model = Some(model.to_string());
        } else {
            self.default_options = Some(Options {
                embed_model: Some(model.to_string()),
                ..Default::default()
            });
        }
        self
    }

    pub fn default_dimensions(&mut self, dimensions: u16) -> &mut Self {
        if let Some(options) = self.default_options.as_mut() {
            if let Some(model) = &options.embed_model {
                let embed_model: QwenEmbedding = model.into();
                match embed_model {
                    QwenEmbedding::TextEmbeddingV1 => assert_eq!(
                        dimensions, 1536,
                        "Dimensions must be 1536 for this embedding model"
                    ),
                    QwenEmbedding::TextEmbeddingV2 => assert_eq!(
                        dimensions, 1536,
                        "Dimensions must be 1536 for this embedding model"
                    ),
                    QwenEmbedding::TextEmbeddingV3 => assert!(
                        matches!(dimensions, 1024 | 768 | 512),
                        "Dimensions must be one of [1024, 768, 512] for TextEmbeddingV3"
                    ),
                    QwenEmbedding::TextEmbeddingAsyncV1 => assert_eq!(
                        dimensions, 1536,
                        "Dimensions must be 1536 for this embedding model"
                    ),
                    QwenEmbedding::TextEmbeddingAsyncV2 => assert_eq!(
                        dimensions, 1536,
                        "Dimensions must be 1536 for this embedding model"
                    ),
                }
            }

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
        let openai = Qwen::builder()
            .default_prompt_model(&QwenModel::Long)
            .build()
            .unwrap();
        assert_eq!(
            openai.default_options.prompt_model,
            Some(QwenModel::Long.to_string())
        );

        let openai = Qwen::builder()
            .default_prompt_model(&QwenModel::Turbo)
            .build()
            .unwrap();
        assert_eq!(
            openai.default_options.prompt_model,
            Some(QwenModel::Turbo.to_string())
        );
    }
}
