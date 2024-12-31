use std::borrow::Cow;

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
pub enum Template<'inner> {
    /// A reference to a compiled template stored in the template repository
    /// These can also be created on the fly with `Template::try_compiled_from_str`,
    /// or retrieved at runtime with `Template::from_compiled_template_name`
    CompiledTemplate(Cow<'inner, str>),

    /// A one-off template that is not stored in the repository
    OneOff(Cow<'inner, str>),
}

impl<'inner> Template<'inner> {
    /// Creates a reference to a template already stored in the repository
    pub fn from_compiled_template_name(name: impl Into<Cow<'inner, str>>) -> Template<'inner> {
        Template::CompiledTemplate(name.into())
    }

    pub fn from_string(template: impl Into<Cow<'inner, str>>) -> Template<'inner> {
        Template::OneOff(template.into())
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
    ) -> Result<Template<'inner>> {
        let id = Uuid::new_v4().to_string();
        let mut lock = TEMPLATE_REPOSITORY.write().await;
        lock.add_raw_template(&id, template.as_ref())
            .context("Failed to add raw template")?;

        Ok(Template::CompiledTemplate(id.into()))
    }

    /// Renders a template with an optional `tera::Context`
    ///
    /// # Errors
    ///
    /// - Template cannot be found
    /// - One-off template has errors
    /// - Context is missing that is required by the template
    pub async fn render(&self, context: &tera::Context) -> Result<String> {
        use Template::{CompiledTemplate, OneOff};

        let template = match self {
            CompiledTemplate(id) => {
                let lock = TEMPLATE_REPOSITORY.read().await;
                tracing::debug!(
                    ?id,
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
            OneOff(template) => Tera::one_off(template, context, false)
                .context("Failed to render one-off template")?,
        };
        Ok(template)
    }
}

impl Template<'_> {
    /// Creates an owned version of the template
    ///
    // NOTE: std ToOwned and Clone preserve the Cow types, which is not what we want
    pub fn to_owned(&self) -> Template<'static> {
        match self {
            Template::CompiledTemplate(template) => {
                Template::CompiledTemplate(template.clone().into_owned().into())
            }
            Template::OneOff(template) => Template::OneOff(template.clone().into_owned().into()),
        }
    }

    /// Builds a Prompt from a template with an empty context
    pub fn to_prompt(&self) -> Prompt {
        Prompt::from(self.to_owned())
    }
}

impl<'inner> From<&'inner str> for Template<'inner> {
    fn from(template: &'inner str) -> Self {
        Template::OneOff(template.into())
    }
}

impl From<String> for Template<'_> {
    fn from(template: String) -> Self {
        Template::OneOff(template.into())
    }
}
