//! Generic delegation tool that enables the agent to delegate tasks to other agents.
use std::borrow::Cow;

use async_trait::async_trait;
use derive_builder::Builder;
use swiftide_core::{
    chat_completion::{self, errors::ToolError, ToolOutput, ToolSpec},
    AgentContext, Tool,
};


#[derive(Clone, Builder)]
pub struct TaskCompleted {
    tool_spec: ToolSpec,
}

impl TaskCompleted {
    #[must_use]
    pub fn builder() -> TaskCompletedBuilder {
        TaskCompletedBuilder::default()
    }

    pub fn task_completed(&self, _context: &dyn AgentContext) -> Result<ToolOutput, ToolError> {
        // task_complete()

        // NOTE: We can make stopping optional, that's pretty cool
        tracing::info!("Delegated task to agent");
        Ok(ToolOutput::Stop)
    }
}

#[async_trait]
impl Tool for TaskCompleted {
    async fn invoke(
        &self,
        agent_context: &dyn AgentContext,
        _raw_args: Option<&str>,
    ) -> Result<ToolOutput, ToolError> {
        return self.task_completed(agent_context);
    }

    fn tool_spec(&self) -> chat_completion::ToolSpec {
        self.tool_spec.clone()
    }

    fn name(&self) -> Cow<'_, str> {
        self.tool_spec().name.into()
    }
}

pub fn default_complete_toolspec(tool_name: &str) -> ToolSpec {
    ToolSpec::builder()
        .name(tool_name)
        .description("Marks the task as completed")
        .build()
        .expect("infallible; failed to build default complete tool spec")
}
