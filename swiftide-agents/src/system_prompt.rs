//! The system prompt is the initial role and constraint defining message the LLM will receive for
//! completion.
//!
//! By default, the system prompt is setup as a general-purpose chain-of-thought reasoning prompt
//! with the role, guidelines, and constraints left empty for customization.
//!
//! You can override the the template entirely by providing your own `Prompt`. Optionally, you can
//! still use the builder values by referencing them in your template.
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

    /// Optional additional raw markdown to append to the prompt
    ///
    /// For instance, if you would like to support an AGENTS.md file, add it here.
    #[builder(default)]
    additional: Option<String>,

    /// The template to use for the system prompt
    #[builder(default = default_prompt_template())]
    template: Prompt,
}

impl SystemPrompt {
    pub fn builder() -> SystemPromptBuilder {
        SystemPromptBuilder::default()
    }

    pub fn to_prompt(&self) -> Prompt {
        self.clone().into()
    }
}

impl From<String> for SystemPrompt {
    fn from(text: String) -> Self {
        SystemPrompt {
            role: None,
            guidelines: Vec::new(),
            constraints: Vec::new(),
            additional: None,
            template: text.into(),
        }
    }
}

impl From<&'static str> for SystemPrompt {
    fn from(text: &'static str) -> Self {
        SystemPrompt {
            role: None,
            guidelines: Vec::new(),
            constraints: Vec::new(),
            additional: None,
            template: text.into(),
        }
    }
}

impl From<SystemPrompt> for SystemPromptBuilder {
    fn from(val: SystemPrompt) -> Self {
        SystemPromptBuilder {
            role: Some(val.role),
            guidelines: Some(val.guidelines),
            constraints: Some(val.constraints),
            additional: Some(val.additional),
            template: Some(val.template),
        }
    }
}

impl From<Prompt> for SystemPrompt {
    fn from(prompt: Prompt) -> Self {
        SystemPrompt {
            role: None,
            guidelines: Vec::new(),
            constraints: Vec::new(),
            additional: None,
            template: prompt,
        }
    }
}

impl Default for SystemPrompt {
    fn default() -> Self {
        SystemPrompt {
            role: None,
            guidelines: Vec::new(),
            constraints: Vec::new(),
            additional: None,
            template: default_prompt_template(),
        }
    }
}

impl SystemPromptBuilder {
    pub fn add_guideline(&mut self, guideline: &str) -> &mut Self {
        self.guidelines
            .get_or_insert_with(Vec::new)
            .push(guideline.to_string());
        self
    }

    pub fn add_constraint(&mut self, constraint: &str) -> &mut Self {
        self.constraints
            .get_or_insert_with(Vec::new)
            .push(constraint.to_string());
        self
    }

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
            additional,
        } = self;

        template
            .with_context_value("role", role)
            .with_context_value("guidelines", guidelines)
            .with_context_value("constraints", constraints)
            .with_context_value("additional", additional)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_customization() {
        let prompt = SystemPrompt::builder()
            .role("special role")
            .guidelines(["special guideline"])
            .constraints(vec!["special constraint".to_string()])
            .additional("some additional info")
            .build()
            .unwrap();

        let prompt: Prompt = prompt.into();

        let rendered = prompt.render().unwrap();

        assert!(rendered.contains("special role"), "error: {rendered}");
        assert!(rendered.contains("special guideline"), "error: {rendered}");
        assert!(rendered.contains("special constraint"), "error: {rendered}");
        assert!(
            rendered.contains("some additional info"),
            "error: {rendered}"
        );

        insta::assert_snapshot!(rendered);
    }

    #[tokio::test]
    async fn test_to_prompt() {
        let prompt = SystemPrompt::builder()
            .role("special role")
            .guidelines(["special guideline"])
            .constraints(vec!["special constraint".to_string()])
            .additional("some additional info")
            .build()
            .unwrap();

        let prompt: Prompt = prompt.to_prompt();

        let rendered = prompt.render().unwrap();

        assert!(rendered.contains("special role"), "error: {rendered}");
        assert!(rendered.contains("special guideline"), "error: {rendered}");
        assert!(rendered.contains("special constraint"), "error: {rendered}");
        assert!(
            rendered.contains("some additional info"),
            "error: {rendered}"
        );

        insta::assert_snapshot!(rendered);
    }

    #[tokio::test]
    async fn test_system_prompt_to_builder() {
        let sp = SystemPrompt {
            role: Some("Assistant".to_string()),
            guidelines: vec!["Be concise".to_string()],
            constraints: vec!["No personal opinions".to_string()],
            additional: None,
            template: "Hello, {{role}}! Guidelines: {{guidelines}}, Constraints: {{constraints}}"
                .into(),
        };

        let builder = SystemPromptBuilder::from(sp.clone());

        assert_eq!(builder.role, Some(Some("Assistant".to_string())));
        assert_eq!(builder.guidelines, Some(vec!["Be concise".to_string()]));
        assert_eq!(
            builder.constraints,
            Some(vec!["No personal opinions".to_string()])
        );
        // For template, compare the rendered string
        assert_eq!(
            builder.template.as_ref().unwrap().render().unwrap(),
            sp.template.render().unwrap()
        );
    }
}
