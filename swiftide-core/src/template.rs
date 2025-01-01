use anyhow::{Context as _, Result};
use tokio::sync::RwLock;

use lazy_static::lazy_static;
use tera::Tera;
use uuid::Uuid;

use crate::prompt::Prompt;

lazy_static! {
    /// Tera repository for templates
    static ref TEMPLATE_REPOSITORY: RwLock<Tera> = {
        let prefix = env!("CARGO_MANIFEST_DIR");
        let path = format!("{prefix}/src/transformers/prompts/**/*.prompt.md");

        match Tera::new(&path)
        {
            Ok(t) => RwLock::new(t),
            Err(e) => {
                tracing::error!("Parsing error(s): {e}");
                ::std::process::exit(1);
            }
        }
    };
}
/// A `Template` defines a template for a prompt
#[derive(Clone, Debug)]
pub enum Template {
    CompiledTemplate(String),
    String(String),
    Static(&'static str),
}

impl Template {
    /// Creates a reference to a template already stored in the repository
    pub fn from_compiled_template_name(name: impl Into<String>) -> Template {
        Template::CompiledTemplate(name.into())
    }

    pub fn from_string(template: impl Into<String>) -> Template {
        Template::String(template.into())
    }

    /// Extends the prompt repository with a custom [`tera::Tera`] instance.
    ///
    /// If you have your own prompt templates or want to add other functionality, you can extend
    /// the repository with your own [`tera::Tera`] instance.
    ///
    /// WARN: Do not use this inside a pipeline or any form of load, as it will lock the repository
    ///
    /// # Errors
    ///
    /// Errors if the repository could not be extended
    pub async fn extend(tera: &Tera) -> Result<()> {
        TEMPLATE_REPOSITORY
            .write()
            .await
            .extend(tera)
            .context("Could not extend prompt repository with custom Tera instance")
    }

    /// Compiles a template from a string and returns a `Template` with a reference to the
    /// string.
    ///
    /// WARN: Do not use this inside a pipeline or any form of load, as it will lock the repository
    ///
    /// # Errors
    ///
    /// Errors if the template fails to compile
    pub async fn try_compiled_from_str(
        template: impl AsRef<str> + Send + 'static,
    ) -> Result<Template> {
        let id = Uuid::new_v4().to_string();
        let mut lock = TEMPLATE_REPOSITORY.write().await;
        lock.add_raw_template(&id, template.as_ref())
            .context("Failed to add raw template")?;

        Ok(Template::CompiledTemplate(id))
    }

    /// Renders a template with an optional `tera::Context`
    ///
    /// # Errors
    ///
    /// - Template cannot be found
    /// - One-off template has errors
    /// - Context is missing that is required by the template
    pub async fn render(&self, context: &tera::Context) -> Result<String> {
        use Template::{CompiledTemplate, Static, String};

        let template = match self {
            CompiledTemplate(id) => {
                let lock = TEMPLATE_REPOSITORY.read().await;
                tracing::debug!(
                    id,
                    available = ?lock.get_template_names().collect::<Vec<_>>(),
                    "Rendering template ..."
                );
                let result = lock.render(id, context);

                if result.is_err() {
                    tracing::error!(
                        error = result.as_ref().unwrap_err().to_string(),
                        available = ?lock.get_template_names().collect::<Vec<_>>(),
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

    /// Builds a Prompt from a template with an empty context
    pub fn to_prompt(&self) -> Prompt {
        self.into()
    }
}

impl From<&'static str> for Template {
    fn from(template: &'static str) -> Self {
        Template::Static(template)
    }
}

impl From<String> for Template {
    fn from(template: String) -> Self {
        Template::String(template)
    }
}
