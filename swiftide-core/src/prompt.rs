//! Prompts templating and management
//!
//! Prompts are first class citizens in Swiftide and use [tera] under the hood. tera
//! uses jinja style templates which allows for a lot of flexibility.
//!
//! Conceptually, a [Prompt] is something you send to i.e.
//! [`SimplePrompt`][crate::SimplePrompt]. A prompt can have
//! added context for substitution and other templating features.
//!
//! Transformers in Swiftide come with default prompts, and they can be customized or replaced as
//! needed.
//!
//! [`Template`] can be added with [`Template::try_compiled_from_str`]. Prompts can also be
//! created on the fly from anything that implements [`Into<String>`]. Compiled prompts are stored
//! in an internal repository.
//!
//! Additionally, `Template::String` and `Template::Static` can be used to create
//! templates on the fly as well.
//!
//! It's recommended to precompile your templates.
//!
//! # Example
//!
//! ```
//! #[tokio::main]
//! # async fn main() {
//! # use swiftide_core::template::Template;
//! let template = Template::try_compiled_from_str("hello {{world}}").await.unwrap();
//! let prompt = template.to_prompt().with_context_value("world", "swiftide");
//!
//! assert_eq!(prompt.render().await.unwrap(), "hello swiftide");
//! # }
//! ```
use std::{
    borrow::Cow,
    sync::{LazyLock, RwLock},
};

use anyhow::{Context as _, Result};
use tera::Tera;

use crate::node::Node;

/// A Prompt can be used with large language models to prompt.
#[derive(Clone, Debug)]
pub struct Prompt {
    template_ref: TemplateRef,
    context: Option<tera::Context>,
}

/// References a to be rendered template
/// Either a one-off template or a tera template
#[derive(Clone, Debug)]
enum TemplateRef {
    OneOff(String),
    Tera(String),
}

pub static SWIFTIDE_TERA: LazyLock<RwLock<Tera>> = LazyLock::new(|| RwLock::new(Tera::default()));

impl Prompt {
    /// Extend the swiftide repository with another Tera instance.
    ///
    /// You can use this to add your own templates, functions and partials.
    ///
    /// # Panics
    ///
    /// Panics if the `RWLock` is poisoned.
    ///
    /// # Errors
    ///
    /// Errors if the `Tera` instance cannot be extended.
    pub fn extend(other: &Tera) -> Result<()> {
        let mut swiftide_tera = SWIFTIDE_TERA.write().unwrap();
        swiftide_tera.extend(other)?;
        Ok(())
    }

    /// Create a new prompt from a compiled template that is present in the Tera repository
    pub fn from_compiled_template(name: impl Into<String>) -> Prompt {
        Prompt {
            template_ref: TemplateRef::Tera(name.into()),
            context: None,
        }
    }

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
    /// If no context is provided, the prompt will be rendered as is.
    ///
    /// # Errors
    ///
    /// See `Template::render`
    ///
    /// # Panics
    ///
    /// Panics if the `RWLock` is poisoned.
    pub fn render(&self) -> Result<String> {
        if self.context.is_none() {
            if let TemplateRef::OneOff(ref template) = self.template_ref {
                return Ok(template.to_string());
            }
        }

        let context: Cow<'_, tera::Context> = self
            .context
            .as_ref()
            .map_or_else(|| Cow::Owned(tera::Context::default()), Cow::Borrowed);

        match &self.template_ref {
            TemplateRef::OneOff(template) => {
                tera::Tera::one_off(template.as_ref(), &context, false)
                    .context("Failed to render one-off template")
            }
            TemplateRef::Tera(ref template) => SWIFTIDE_TERA
                .read()
                .unwrap()
                .render(template.as_ref(), &context)
                .context("Failed to render template"),
        }
    }
}

impl From<&str> for Prompt {
    fn from(prompt: &str) -> Self {
        Prompt {
            template_ref: TemplateRef::OneOff(prompt.into()),
            context: None,
        }
    }
}

impl From<String> for Prompt {
    fn from(prompt: String) -> Self {
        Prompt {
            template_ref: TemplateRef::OneOff(prompt),
            context: None,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn test_prompt() {
        let prompt: Prompt = "hello {{world}}".into();
        let prompt = prompt.with_context_value("world", "swiftide");
        assert_eq!(prompt.render().unwrap(), "hello swiftide");
    }

    #[tokio::test]
    async fn test_prompt_with_node() {
        let prompt: Prompt = "hello {{node.chunk}}".into();
        let node = Node::new("test");
        let prompt = prompt.with_node(&node);
        assert_eq!(prompt.render().unwrap(), "hello test");
    }

    #[tokio::test]
    async fn test_one_off_from_string() {
        let mut prompt: Prompt = "hello {{world}}".into();
        prompt = prompt.with_context_value("world", "swiftide");

        assert_eq!(prompt.render().unwrap(), "hello swiftide");
    }

    #[tokio::test]
    async fn test_extending_with_custom_repository() {
        let mut custom_tera = tera::Tera::new("**/some/prompts.md").unwrap();

        custom_tera
            .add_raw_template("hello", "hello {{world}}")
            .unwrap();

        Prompt::extend(&custom_tera).unwrap();

        let prompt =
            Prompt::from_compiled_template("hello").with_context_value("world", "swiftide");

        assert_eq!(prompt.render().unwrap(), "hello swiftide");
    }

    #[tokio::test]
    async fn test_coercion_to_prompt() {
        // str
        let raw: &str = "hello {{world}}";

        let prompt: Prompt = raw.into();
        assert_eq!(
            prompt
                .with_context_value("world", "swiftide")
                .render()
                .unwrap(),
            "hello swiftide"
        );

        let prompt: Prompt = raw.to_string().into();
        assert_eq!(
            prompt
                .with_context_value("world", "swiftide")
                .render()
                .unwrap(),
            "hello swiftide"
        );
    }

    #[tokio::test]
    async fn test_assume_rendered_unless_context_methods_called() {
        let prompt = Prompt::from("hello {{world}}");

        assert_eq!(prompt.render().unwrap(), "hello {{world}}");
    }
}
