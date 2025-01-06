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
//! created on the fly from anything that implements [`Into<String>`]. Compiled prompts are stored in
//! an internal repository.
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
use anyhow::Result;

use crate::{node::Node, template::Template};

/// A Prompt can be used with large language models to prompt.
#[derive(Clone, Debug)]
pub struct Prompt {
    template: Template,
    context: Option<tera::Context>,
}

#[deprecated(
    since = "0.16.0",
    note = "Use `Template` instead; they serve a more general purpose"
)]
pub type PromptTemplate = Template;

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
    /// If no context is provided, the prompt will be rendered as is.
    ///
    /// # Errors
    ///
    /// See `Template::render`
    pub async fn render(&self) -> Result<String> {
        if let Some(context) = &self.context {
            self.template.render(context).await
        } else {
            match &self.template {
                Template::CompiledTemplate(_) => {
                    self.template.render(&tera::Context::default()).await
                }
                Template::String(string) => Ok(string.clone()),
                Template::Static(string) => Ok((*string).to_string()),
            }
        }
    }
}

impl From<&'static str> for Prompt {
    fn from(prompt: &'static str) -> Self {
        Prompt {
            template: Template::Static(prompt),
            context: None,
        }
    }
}

impl From<String> for Prompt {
    fn from(prompt: String) -> Self {
        Prompt {
            template: Template::String(prompt),
            context: None,
        }
    }
}

impl From<&Template> for Prompt {
    fn from(template: &Template) -> Self {
        Prompt {
            template: template.clone(),
            context: None,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn test_prompt() {
        let template = Template::try_compiled_from_str("hello {{world}}")
            .await
            .unwrap();
        let prompt = template.to_prompt().with_context_value("world", "swiftide");
        assert_eq!(prompt.render().await.unwrap(), "hello swiftide");
    }

    #[tokio::test]
    async fn test_prompt_with_node() {
        let template = Template::try_compiled_from_str("hello {{node.chunk}}")
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
        let mut custom_tera = tera::Tera::new("**/some/prompts.md").unwrap();

        custom_tera
            .add_raw_template("hello", "hello {{world}}")
            .unwrap();

        Template::extend(&custom_tera).await.unwrap();

        let prompt = Template::from_compiled_template_name("hello")
            .to_prompt()
            .with_context_value("world", "swiftide");

        assert_eq!(prompt.render().await.unwrap(), "hello swiftide");
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
                .await
                .unwrap(),
            "hello swiftide"
        );

        let prompt: Prompt = raw.to_string().into();
        assert_eq!(
            prompt
                .with_context_value("world", "swiftide")
                .render()
                .await
                .unwrap(),
            "hello swiftide"
        );
    }

    #[tokio::test]
    async fn test_coercion_to_template() {
        let raw: &str = "hello {{world}}";

        let prompt: Template = raw.into();
        assert_eq!(
            prompt
                .to_prompt()
                .with_context_value("world", "swiftide")
                .render()
                .await
                .unwrap(),
            "hello swiftide"
        );

        let prompt: Template = raw.to_string().into();
        assert_eq!(
            prompt
                .to_prompt()
                .with_context_value("world", "swiftide")
                .render()
                .await
                .unwrap(),
            "hello swiftide"
        );
    }

    #[tokio::test]
    async fn test_assume_rendered_unless_context_methods_called() {
        let prompt = Prompt::from("hello {{world}}");

        assert_eq!(prompt.render().await.unwrap(), "hello {{world}}");
    }
}
