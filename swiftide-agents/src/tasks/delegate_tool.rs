use std::{borrow::Cow, sync::Arc};

use async_trait::async_trait;
use derive_builder::Builder;
use serde::Deserialize;
use swiftide_core::{
    chat_completion::{self, errors::ToolError, ToolOutput, ToolSpec},
    AgentContext, Tool,
};

use super::task::Task;

#[derive(Clone, Builder)]
pub struct DelegateAgent {
    task: Task,
    delegates_to_agent: String,

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
        // TODO: Should we figure out a way to just stop the agent right here?
        // Or run the agent non blockin parallel?
        self.task.swap_active_agent(&self.agent)?;
        self.task.invoke(instructions).await?;

        tracing::info!("Delegated task to agent");
        Ok(ToolOutput::Stop)
    }
}

#[derive(Deserialize)]
struct DelegateArgs {
    task: String,
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

        let args: DelegateArgs = serde_json::from_str(&args)?;
        return self.delegate_agent(agent_context, &args.task).await;
    }

    fn tool_spec(&self) -> chat_completion::ToolSpec {
        self.tool_spec.clone()
    }

    fn name(&self) -> Cow<'_, str> {
        self.tool_spec().name.into()
    }
}
