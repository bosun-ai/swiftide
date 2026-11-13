//! What an agent does when two of its toolboxes provide a tool with the same name.
use std::borrow::Cow;

use async_trait::async_trait;
use swiftide_core::{
    AgentContext, Tool, ToolBox,
    chat_completion::{ToolCall, ToolOutput, ToolSpec, errors::ToolError},
};

/// What an agent does when two of its toolboxes provide a tool with the same name.
///
/// Only clashes between toolboxes count. A tool added directly with `AgentBuilder::tools` always
/// wins over a toolbox tool with the same name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DuplicateTools {
    /// Keep the tool from the toolbox that was added first, and log the others.
    #[default]
    KeepFirst,
    /// Refuse to start the agent with `AgentError::ToolBoxFailedToLoad`.
    Fail,
    /// Keep both: the later tool is renamed to `<toolbox name>_<tool name>`, with the toolbox
    /// name reduced to letters, digits, `_` and `-`. The first tool keeps its name.
    Prefix,
}

/// What to do with a toolbox tool whose name is already taken by an earlier toolbox.
pub(crate) enum Resolution {
    Skip,
    Fail(anyhow::Error),
    Register(Box<dyn Tool>),
}

impl DuplicateTools {
    pub(crate) fn resolve(self, tool: Box<dyn Tool>, toolbox: &dyn ToolBox) -> Resolution {
        match self {
            DuplicateTools::KeepFirst => {
                tracing::warn!(
                    tool = %tool.name(),
                    toolbox = %toolbox.name(),
                    "Skipping tool; an earlier toolbox already provides a tool with this name"
                );
                Resolution::Skip
            }
            DuplicateTools::Fail => Resolution::Fail(anyhow::anyhow!(
                "Tool `{}` from toolbox `{}` has the same name as a tool from another toolbox",
                tool.name(),
                toolbox.name()
            )),
            DuplicateTools::Prefix => {
                let name = prefixed_name(&toolbox.name(), &tool.name());
                Resolution::Register(Box::new(PrefixedTool { name, tool }))
            }
        }
    }
}

fn prefixed_name(toolbox: &str, tool: &str) -> String {
    let prefix: String = toolbox
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{prefix}_{tool}")
}

/// A toolbox tool registered under another name, so it does not clash with an earlier toolbox.
#[derive(Clone)]
struct PrefixedTool {
    name: String,
    tool: Box<dyn Tool>,
}

#[async_trait]
impl Tool for PrefixedTool {
    async fn invoke(
        &self,
        agent_context: &dyn AgentContext,
        tool_call: &ToolCall,
    ) -> Result<ToolOutput, ToolError> {
        self.tool.invoke(agent_context, tool_call).await
    }

    fn name(&self) -> Cow<'_, str> {
        self.name.as_str().into()
    }

    fn tool_spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name.clone(),
            ..self.tool.tool_spec()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolbox_names_become_safe_tool_name_prefixes() {
        assert_eq!(
            prefixed_name(" Unnamed ToolBox ", "search"),
            "Unnamed_ToolBox_search"
        );
        assert_eq!(prefixed_name("git-hub.v2", "search"), "git-hub_v2_search");
    }
}
