//! Prompts templating and management
//!
//! Prompts are first class citizens in Swiftide and use [tera] under the hood. tera
//! uses jinja style templates which allows for a lot of flexibility.
//!
//! Conceptually, a [Prompt] is something you send to i.e.
//! [`SimplePrompt`][crate::traits::SimplePrompt]. A prompt can have
//! added context for substitution and other templating features.
//!
//! Transformers in Swiftide come with default prompts, and they can be customized or replaced as
//! needed.
//!
//! [`PromptTemplate`] can be added with [`PromptTemplate::try_compiled_from_str`]. Prompts can also be
//! created on the fly from anything that implements [`Into<String>`]. Compiled prompts are stored in
//! an internal repository.
//!
//! Additionally, `PromptTemplate::String` and `PromptTemplate::Static` can be used to create
//! templates on the fly as well.
//!
//! It's recommended to precompile your templates.
//!
//! # Example
//!
//! ```
//! #[tokio::main]
//! # async fn main() {
//! # use swiftide::prompt::PromptTemplate;
//! let template = PromptTemplate::try_compiled_from_str("hello {{world}}").await.unwrap();
//! let prompt = template.to_prompt().with_context_value("world", "swiftide");
//!
//! assert_eq!(prompt.render().await.unwrap(), "hello swiftide");
//! # }
//! ```
use anyhow::{Context as _, Result};
use lazy_static::lazy_static;
use tera::Tera;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::ingestion::Node;

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

/// A Prompt can be used with large language models to prompt.
#[derive(Clone, Debug)]
pub struct Prompt {
    template: PromptTemplate,
    context: Option<tera::Context>,
}

/// A `PromptTemplate` defines a template for a prompt
#[derive(Clone, Debug)]
pub enum PromptTemplate {
    CompiledTemplate(String),
    String(String),
    Static(&'static str),
}

impl<'tmpl> PromptTemplate {
    /// Creates a reference to a template stored in the repository
    pub fn from_compiled_template_name(name: impl Into<String>) -> PromptTemplate {
        PromptTemplate::CompiledTemplate(name.into())
    }

    /// Extends the prompt repository with a custom [`terra::Tera`] instance.
    ///
    /// If you have your own prompt templates or want to add other functionality, you can extend
    /// the repository with your own [`terra::Tera`] instance.
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

    /// Compiles a template from a string and returns a `PromptTemplate` with a reference to the
    /// string.
    ///
    /// WARN: Do not use this inside a pipeline or any form of load, as it will lock the repository
    ///
    /// # Errors
    ///
    /// Errors if the template fails to compile
    pub async fn try_compiled_from_str(
        template: impl AsRef<str> + Send + 'static,
    ) -> Result<PromptTemplate> {
        let id = Uuid::new_v4().to_string();
        let mut lock = TEMPLATE_REPOSITORY.write().await;
        lock.add_raw_template(&id, template.as_ref())
            .context("Failed to add raw template")?;

        Ok(PromptTemplate::CompiledTemplate(id))
    }

    /// Renders a template with an optional `tera::Context`
    ///
    /// # Errors
    ///
    /// - Template cannot be found
    /// - One-off template has errors
    /// - Context is missing that is required by the template
    pub async fn render(&self, context: &Option<tera::Context>) -> Result<String> {
        use PromptTemplate::{CompiledTemplate, Static, String};

        let template = match self {
            CompiledTemplate(id) => {
                let context = match &context {
                    Some(context) => context,
                    None => &tera::Context::default(),
                };

                let lock = TEMPLATE_REPOSITORY.read().await;
                let available = lock.get_template_names().collect::<Vec<_>>().join(", ");
                tracing::debug!(id, available, "Rendering template ...");
                let result = lock.render(id, context);

                if result.is_err() {
                    tracing::error!(
                        error = result.as_ref().unwrap_err().to_string(),
                        available,
                        "Error rendering template {id}"
                    );
                }
                result.with_context(|| format!("Failed to render template '{id}'"))?
            }
            String(template) => {
                if let Some(context) = context {
                    Tera::one_off(template, context, false)
                        .context("Failed to render one-off template")?
                } else {
                    template.to_string()
                }
            }
            Static(template) => {
                if let Some(context) = context {
                    Tera::one_off(template, context, false)
                        .context("Failed to render one-off template")?
                } else {
                    (*template).to_string()
                }
            }
        };
        Ok(template)
    }

    /// Builds a Prompt from a template with an empty context
    pub fn to_prompt(&self) -> Prompt {
        Prompt {
            template: self.clone(),
            context: Some(tera::Context::default()),
        }
    }
}

impl Prompt {
    /// Adds an `ingestion::Node` to the context of the Prompt
    #[must_use]
    pub fn with_node(mut self, node: &Node) -> Self {
        let context = self.context.get_or_insert_with(tera::Context::default);
        context.insert("node", &node);
        self
    }

    /// Adds anything that implements [Into<tera::Context>], like `Serialize` to the Prompt
    #[must_use]
    pub fn with_context(mut self, new_context: impl Into<tera::Context>) -> Self {
        let context = self.context.get_or_insert_with(tera::Context::default);
        context.extend(new_context.into());

        self
    }

    /// Adds a key-value pair to the context of the Prompt
    #[must_use]
    pub fn with_context_value(mut self, key: &str, value: impl Into<tera::Value>) -> Self {
        let context = self.context.get_or_insert_with(tera::Context::default);
        context.insert(key, &value.into());
        self
    }

    /// Renders a prompt
    ///
    /// # Errors
    ///
    /// See `PromptTemplate::render`
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

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn test_prompt() {
        let template = PromptTemplate::try_compiled_from_str("hello {{world}}")
            .await
            .unwrap();
        let prompt = template.to_prompt().with_context_value("world", "swiftide");
        assert_eq!(prompt.render().await.unwrap(), "hello swiftide");
    }

    #[tokio::test]
    async fn test_prompt_with_node() {
        let template = PromptTemplate::try_compiled_from_str("hello {{node.chunk}}")
            .await
            .unwrap();
        let node = Node::new("test");
        let prompt = template.to_prompt().with_node(&node);
        assert_eq!(prompt.render().await.unwrap(), "hello test");
    }

    #[tokio::test]
    async fn test_one_off_from_string() {
        let mut prompt: Prompt = "hello {{world}}".into();
        prompt = prompt.with_context_value("world", "swiftide");

        assert_eq!(prompt.render().await.unwrap(), "hello swiftide");
    }

    #[tokio::test]
    async fn test_extending_with_custom_repository() {
        let mut custom_tera = Tera::new("**/some/prompts.md").unwrap();

        custom_tera
            .add_raw_template("hello", "hello {{world}}")
            .unwrap();

        PromptTemplate::extend(&custom_tera).await.unwrap();

        let prompt = PromptTemplate::from_compiled_template_name("hello")
            .to_prompt()
            .with_context_value("world", "swiftide");

        assert_eq!(prompt.render().await.unwrap(), "hello swiftide");
    }
}
