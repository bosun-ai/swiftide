use anyhow::{Context as _, Result};
use lazy_static::lazy_static;
use tera::Tera;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::ingestion::Node;

lazy_static! {
    pub static ref TEMPLATES: RwLock<Tera> = {
        match Tera::new("**/*.prompt.md") {
            Ok(t) => RwLock::new(t),
            Err(e) => {
                tracing::error!("Parsing error(s): {e}");
                ::std::process::exit(1);
            }
        }
    };
}

#[derive(Clone, Debug)]
pub struct Prompt {
    template: PromptTemplate,
    context: Option<tera::Context>,
}

#[derive(Clone, Debug)]
pub enum PromptTemplate {
    CompiledTemplate(String),
    String(String),
    Static(&'static str),
}

impl<'tmpl> PromptTemplate {
    pub fn from_compiled_template_name(name: impl Into<String>) -> PromptTemplate {
        PromptTemplate::CompiledTemplate(name.into())
    }

    pub async fn try_compiled_from_str(
        template: impl AsRef<str> + Send + 'static,
    ) -> Result<PromptTemplate> {
        let id = Uuid::new_v4().to_string();
        let mut lock = TEMPLATES.write().await;
        lock.add_raw_template(&id, template.as_ref())
            .context("Failed to add raw template")?;

        Ok(PromptTemplate::CompiledTemplate(id))
    }

    pub async fn render(&self, context: &Option<tera::Context>) -> Result<String> {
        use PromptTemplate::{CompiledTemplate, Static, String};

        let context = match &context {
            Some(context) => context,
            None => &tera::Context::default(),
        };

        let template = match self {
            CompiledTemplate(id) => {
                let lock = TEMPLATES.read().await;
                let available = lock.get_template_names().collect::<Vec<_>>().join(", ");
                tracing::debug!(id, available, "Rendering template ...");
                let result = lock.render(id, context);

                if result.is_err() {
                    tracing::error!(
                        error = result.as_ref().unwrap_err().to_string(),
                        "Error rendering template {id}"
                    );
                }
                result.with_context(|| format!("Failed to render template '{id}'"))?
            }
            String(template) => Tera::one_off(template, context, false)
                .context("Failed to render one-off template")?,
            Static(template) => Tera::one_off(template, context, false)
                .context("Failed to render one-off template")?,
        };
        Ok(template)
    }

    pub fn to_prompt(&self) -> Prompt {
        Prompt {
            template: self.clone(),
            context: Some(tera::Context::default()),
        }
    }
}

impl Prompt {
    pub fn with_node(mut self, node: &Node) -> Self {
        let context = self.context.get_or_insert_with(tera::Context::default);
        context.insert("node", &node);
        self
    }

    pub fn with_context(mut self, new_context: impl Into<tera::Context>) -> Self {
        let context = self.context.get_or_insert_with(tera::Context::default);
        context.extend(new_context.into());

        self
    }

    pub fn with_context_value(mut self, key: &str, value: impl Into<tera::Value>) -> Self {
        let context = self.context.get_or_insert_with(tera::Context::default);
        context.insert(key, &value.into());
        self
    }

    pub async fn render(&self) -> Result<String> {
        self.template.render(&self.context).await
    }
}

impl From<&'static str> for Prompt {
    fn from(prompt: &'static str) -> Self {
        Prompt {
            template: PromptTemplate::Static(prompt),
            context: None,
        }
    }
}

impl From<String> for Prompt {
    fn from(prompt: String) -> Self {
        Prompt {
            template: PromptTemplate::String(prompt),
            context: None,
        }
    }
}
