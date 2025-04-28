//! Generic delegation tool that enables the agent to delegate tasks to other agents.
use std::borrow::Cow;

use anyhow::Context as _;
use async_trait::async_trait;
use derive_builder::Builder;
use serde::Deserialize;
use swiftide_core::{
    chat_completion::{self, errors::ToolError, ParamSpec, ToolOutput, ToolSpec},
    AgentContext, Tool,
};

use super::{running_agent::RunningAgent, task::Task};

#[derive(Clone, Builder)]
pub struct DelegateAgent {
    // TODO: Might be possible to borrow task/running agent (event though cheap to clone)
    task: Task,
    delegates_to: RunningAgent,

    tool_spec: ToolSpec,
}

impl DelegateAgent {
    #[must_use]
    pub fn builder() -> DelegateAgentBuilder {
        DelegateAgentBuilder::default()
    }

    pub async fn delegate_agent(
        &self,
        _context: &dyn AgentContext,
        instructions: &str,
    ) -> Result<ToolOutput, ToolError> {
        self.task
            .switch_to_agent(&self.delegates_to)
            .await
            .map_err(anyhow::Error::from)?;

        // TODO: Should be a proper error
        self.task
            .invoke(instructions)
            .await
            .context("Failed to invoke task")?;

        // NOTE: We can make stopping optional, that's pretty cool
        tracing::info!("Delegated task to agent");
        Ok(ToolOutput::Stop)
    }
}

#[derive(Deserialize)]
struct DelegateArgs {
    instructions: String,
}

#[async_trait]
impl Tool for DelegateAgent {
    async fn invoke(
        &self,
        agent_context: &dyn AgentContext,
        raw_args: Option<&str>,
    ) -> Result<ToolOutput, ToolError> {
        let Some(args) = raw_args else {
            return Err(ToolError::MissingArguments(format!(
                "No arguments provided for {}",
                self.name()
            )));
        };

        let args: DelegateArgs = serde_json::from_str(args)?;
        return self.delegate_agent(agent_context, &args.instructions).await;
    }

    fn tool_spec(&self) -> chat_completion::ToolSpec {
        self.tool_spec.clone()
    }

    fn name(&self) -> Cow<'_, str> {
        self.tool_spec().name.into()
    }
}

pub fn default_delegate_toolspec(tool_name: &str) -> ToolSpec {
    ToolSpec::builder()
        .name(tool_name)
        .description("Delegates to another agent")
        .parameters(vec![ParamSpec::builder()
            .name("instructions")
            .description("Detailed instructions for the agent")
            .build()
            .unwrap()])
        .build()
        .expect("infallible; failed to build default delegate tool spec")
}
