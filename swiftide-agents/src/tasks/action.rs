// Desired api
// Desired api:
//
// Action::for_agent("planning_agent")
//     .delegates_to("research_agent")
//     .and_back() // Returns a DelegateAction with and_back set to true
//
// Action::for_agent("jsonspec_agent")
//     .can_complete() // Returns a CompleteAction
//
// If and_back is not provided, the DelegateActionBuilder can be converted
// into an Action (with and_back false) via the From/Into trait.

use serde::{Deserialize, Serialize};
use swiftide_core::{
    chat_completion::{ParamSpec, ToolSpec, ToolSpecBuilderError},
    Tool,
};
use thiserror::Error;

use crate::tasks::delegate_tool::DelegateAgentBuilder;

use super::{
    delegate_tool::DelegateAgentBuilderError,
    task::Task,
    task_completed_tool::{TaskCompleted, TaskCompletedBuilderError},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    Delegate(DelegateAction),
    Complete(CompleteAction),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegateAction {
    pub from_agent: String,
    pub to_agent: String,
    #[serde(default)]
    pub and_back: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteAction {
    pub agent: String,
}

pub struct ActionBuilder {
    pub agent: String,
}

pub struct DelegateActionBuilder {
    pub from_agent: String,
    pub to_agent: String,
    pub and_back: bool,
}

#[derive(Debug, Error)]
pub enum ActionError {
    #[error("Failed to apply action")]
    ApplyFailed,

    #[error("Failed to find agent: {0}")]
    AgentNotFound(String),

    #[error(transparent)]
    FailedBuildingDelegateTool(#[from] DelegateAgentBuilderError),

    #[error(transparent)]
    FailedBuildingCompleteTool(#[from] TaskCompletedBuilderError),

    #[error(transparent)]
    FailedBuildingToolSpec(#[from] ToolSpecBuilderError),
}

impl Action {
    pub fn for_agent<S: Into<String>>(agent: S) -> ActionBuilder {
        ActionBuilder {
            agent: agent.into(),
        }
    }

    /// Applies this action to the task. Tasks apply all their configured actions
    /// after the build.
    pub async fn apply(self, task: &Task) -> Result<(), ActionError> {
        tracing::trace!("Applying action: {:?}", self);
        match self {
            Action::Delegate(delegate_action) => {
                // TODO: Add the task to the tool, also missing toolspec. Defaults with overwrite
                // or required?
                //
                // Build the delegate tool base on the action
                let source = task
                    .find_agent(&delegate_action.from_agent)
                    .await
                    .ok_or_else(|| {
                        ActionError::AgentNotFound(delegate_action.from_agent.clone())
                    })?;
                let target = task
                    .find_agent(&delegate_action.to_agent)
                    .await
                    .ok_or_else(|| ActionError::AgentNotFound(delegate_action.to_agent.clone()))?;

                let tool_spec = ToolSpec::builder()
                    .name("delegate_agent")
                    .description("Delegates to another agent")
                    .parameters(vec![ParamSpec::builder()
                        .name("instructions")
                        .description("Detailed instructions for the agent")
                        .build()
                        .unwrap()])
                    .build()
                    .map_err(ActionError::FailedBuildingToolSpec)?;
                let tool = DelegateAgentBuilder::default()
                    .delegates_to(target.clone())
                    .task(task.clone())
                    .tool_spec(tool_spec.clone())
                    .build()?;

                {
                    let mut source_agent = source.lock().await;
                    source_agent.add_tool(tool.boxed());
                }

                if delegate_action.and_back {
                    let tool = DelegateAgentBuilder::default()
                        .delegates_to(source)
                        .task(task.clone())
                        .tool_spec(tool_spec)
                        .build()?;

                    let mut target_agent = target.lock().await;

                    target_agent.add_tool(tool.boxed());
                }
                Ok(())
            }
            Action::Complete(complete_action) => {
                let tool_spec = ToolSpec::builder()
                    .name("task_completed")
                    .description("Marks the task as completed")
                    .build()
                    .map_err(ActionError::FailedBuildingToolSpec)?;

                let tool = TaskCompleted::builder()
                    .tool_spec(tool_spec)
                    .build()
                    .map_err(ActionError::FailedBuildingCompleteTool)?;

                let agent = task
                    .find_agent(&complete_action.agent)
                    .await
                    .ok_or_else(|| ActionError::AgentNotFound(complete_action.agent.clone()))?;

                agent.lock().await.add_tool(tool.boxed());

                Ok(())
            }
        }
    }
}

impl ActionBuilder {
    pub fn delegates_to<S: Into<String>>(self, to_agent: S) -> DelegateActionBuilder {
        DelegateActionBuilder {
            from_agent: self.agent,
            to_agent: to_agent.into(),
            and_back: false,
        }
    }

    pub fn can_complete(self) -> Action {
        Action::Complete(CompleteAction { agent: self.agent })
    }
}

impl DelegateActionBuilder {
    pub fn and_back(mut self) -> Action {
        self.and_back = true;
        self.into()
    }
}

impl From<DelegateActionBuilder> for Action {
    fn from(builder: DelegateActionBuilder) -> Self {
        Action::Delegate(DelegateAction {
            from_agent: builder.from_agent,
            to_agent: builder.to_agent,
            and_back: builder.and_back,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delegate_action_with_and_back() {
        let planning_agent = "planning_agent";
        let research_agent = "research_agent";

        let action = Action::for_agent(planning_agent)
            .delegates_to(research_agent)
            .and_back();

        match action {
            Action::Delegate(da) => {
                assert_eq!(da.from_agent, planning_agent);
                assert_eq!(da.to_agent, research_agent);
                assert!(da.and_back);
            }
            _ => panic!("Expected a DelegateAction"),
        }
    }

    #[test]
    fn test_delegate_action_without_and_back() {
        let planning_agent = "planning_agent";
        let research_agent = "research_agent";

        // Convert the DelegateActionBuilder into Action with the Into trait.
        let action: Action = Action::for_agent(planning_agent)
            .delegates_to(research_agent)
            .into();

        match action {
            Action::Delegate(da) => {
                assert_eq!(da.from_agent, planning_agent);
                assert_eq!(da.to_agent, research_agent);
                assert!(!da.and_back);
            }
            _ => panic!("Expected a DelegateAction"),
        }
    }

    #[test]
    fn test_complete_action() {
        let jsonspec_agent = "jsonspec_agent";

        let action = Action::for_agent(jsonspec_agent).can_complete();

        match action {
            Action::Complete(ca) => {
                assert_eq!(ca.agent, jsonspec_agent);
            }
            _ => panic!("Expected a CompleteAction"),
        }
    }
}
