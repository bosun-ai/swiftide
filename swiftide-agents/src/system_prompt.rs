//! The system prompt is the initial role and constraint defining message the LLM will receive for
//! completion.
//!
//! The builder provides an accessible way to build a system prompt.
//!
//! The agent will convert the system prompt into a prompt, adding it to the messages list the
//! first time it is called.
//!
//! For customization, either the builder can be used to profit from defaults, or an override can
//! be provided on the agent level.

use derive_builder::Builder;
use swiftide_core::prompt::Prompt;

#[derive(Clone, Debug, Builder)]
#[builder(setter(into, strip_option))]
pub struct SystemPrompt {
    /// The role the agent is expected to fulfil.
    #[builder(default)]
    role: Option<String>,

    /// Additional guidelines for the agent to follow
    #[builder(default, setter(custom))]
    guidelines: Vec<String>,
    /// Additional constraints
    #[builder(default, setter(custom))]
    constraints: Vec<String>,

    /// The template to use for the system prompt
    #[builder(default = default_prompt_template())]
    template: Prompt,
}

impl SystemPrompt {
    pub fn builder() -> SystemPromptBuilder {
        SystemPromptBuilder::default()
    }
}

impl Default for SystemPrompt {
    fn default() -> Self {
        SystemPrompt {
            role: None,
            guidelines: Vec::new(),
            constraints: Vec::new(),
            template: default_prompt_template(),
        }
    }
}

impl SystemPromptBuilder {
    pub fn guidelines<T: IntoIterator<Item = S>, S: AsRef<str>>(
        &mut self,
        guidelines: T,
    ) -> &mut Self {
        self.guidelines = Some(
            guidelines
                .into_iter()
                .map(|s| s.as_ref().to_string())
                .collect(),
        );
        self
    }

    pub fn constraints<T: IntoIterator<Item = S>, S: AsRef<str>>(
        &mut self,
        constraints: T,
    ) -> &mut Self {
        self.constraints = Some(
            constraints
                .into_iter()
                .map(|s| s.as_ref().to_string())
                .collect(),
        );
        self
    }
}

fn default_prompt_template() -> Prompt {
    include_str!("system_prompt_template.md").into()
}

#[allow(clippy::from_over_into)]
impl Into<Prompt> for SystemPrompt {
    fn into(self) -> Prompt {
        let SystemPrompt {
            role,
            guidelines,
            constraints,
            template,
        } = self;

        template
            .with_context_value("role", role)
            .with_context_value("guidelines", guidelines)
            .with_context_value("constraints", constraints)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_customization() {
        let prompt = SystemPrompt::builder()
            .role("role")
            .guidelines(["guideline"])
            .constraints(vec!["constraint".to_string()])
            .build()
            .unwrap();

        let prompt: Prompt = prompt.into();

        let rendered = prompt.render().unwrap();

        insta::assert_snapshot!(rendered);
    }
}
