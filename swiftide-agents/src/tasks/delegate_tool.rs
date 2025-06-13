//! Generic delegation tool that enables the agent to delegate tasks to other agents.
use std::borrow::Cow;

use anyhow::Context as _;
use async_trait::async_trait;
use derive_builder::Builder;
use serde::Deserialize;
use swiftide_core::{
    AgentContext, Tool,
    chat_completion::{self, ParamSpec, ToolCall, ToolOutput, ToolSpec, errors::ToolError},
};

use super::{TaskState, backend::Backend, running_agent::RunningAgent, task::Task};

#[derive(Clone, Builder)]
pub struct DelegateAgent<B: Backend, S: TaskState> {
    // TODO: Might be possible to borrow task/running agent (event though cheap to clone)
    task: Task<B, S>,
    delegates_to: RunningAgent,

    tool_spec: ToolSpec,
}

impl<B: Backend, S: TaskState> DelegateAgent<B, S> {
    #[must_use]
    pub fn builder() -> DelegateAgentBuilder<B, S> {
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
            .query(instructions)
            .await
            .context("Failed to invoke task")?;

        // NOTE: We can make stopping optional, that's pretty cool
        tracing::debug!("Delegated task to agent");
        Ok(ToolOutput::Stop)
    }
}

#[derive(Deserialize)]
struct DelegateArgs {
    instructions: String,
}

#[async_trait]
impl<B: Backend, S: TaskState> Tool for DelegateAgent<B, S> {
    async fn invoke(
        &self,
        agent_context: &dyn AgentContext,
        tool_call: &ToolCall,
    ) -> Result<ToolOutput, ToolError> {
        let Some(args) = tool_call.args() else {
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
        .parameters(vec![
            ParamSpec::builder()
                .name("instructions")
                .description("Detailed instructions for the agent")
                .build()
                .unwrap(),
        ])
        .build()
        .expect("infallible; failed to build default delegate tool spec")
}
