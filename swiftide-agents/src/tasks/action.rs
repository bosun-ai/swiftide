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

use convert_case::{Case, Casing as _};
use serde::{Deserialize, Serialize};
use swiftide_core::{
    Tool,
    chat_completion::{ToolSpec, ToolSpecBuilderError},
};
use thiserror::Error;

use crate::tasks::{
    delegate_tool::DelegateAgentBuilder,
    tools::{default_complete_toolspec, default_delegate_toolspec},
};

use super::{
    TaskState,
    backend::Backend,
    delegate_tool::DelegateAgentBuilderError,
    task::Task,
    task_completed_tool::{TaskCompleted, TaskCompletedBuilderError},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum Action {
    Delegate(DelegateAction),
    Complete(CompleteAction),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct DelegateAction {
    pub from_agent: String,
    pub to_agent: String,
    #[serde(default)]
    pub and_back: bool,

    #[serde(default)]
    pub tool_spec: Option<ToolSpec>,
    #[serde(default)]
    pub back_tool_spec: Option<ToolSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct CompleteAction {
    pub agent: String,
    #[serde(default)]
    pub tool_spec: Option<ToolSpec>,
}

pub struct ActionBuilder {
    pub agent: String,
}

pub struct DelegateActionBuilder {
    pub from_agent: String,
    pub to_agent: String,
    pub and_back: bool,

    /// The tool specification when delegating to another agent
    ///
    /// Note that the tool name must be unique, otherwise it will be overwritten
    pub tool_spec: Option<ToolSpec>,
    /// The tool specification when delegating back to the original agent
    ///
    /// Note that the tool name must be unique, otherwise it will be overwritten
    pub back_tool_spec: Option<ToolSpec>,
}

pub struct CompleteActionBuilder {
    pub agent: String,
    pub tool_spec: Option<ToolSpec>,
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
    ///
    /// # Errors
    ///
    /// Errors if the apply failed, the agent does not exist, or any of the building steps fail
    pub async fn apply<B: Backend, S: TaskState + 'static>(
        self,
        task: &Task<B, S>,
    ) -> Result<(), ActionError> {
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

                let tool_spec = delegate_action.tool_spec.unwrap_or_else(|| {
                    default_delegate_toolspec(&format!(
                        "delegate_{}",
                        delegate_action.from_agent.to_case(Case::Snake)
                    ))
                });

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
                    let tool_spec = delegate_action.back_tool_spec.unwrap_or_else(|| {
                        default_delegate_toolspec(&format!(
                            "delegate_back_{}",
                            delegate_action.to_agent.to_case(Case::Snake)
                        ))
                    });

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
                let tool_spec = complete_action
                    .tool_spec
                    .unwrap_or_else(|| default_complete_toolspec("complete_task"));

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

            tool_spec: None,
            back_tool_spec: None,
        }
    }

    pub fn can_complete(self) -> CompleteActionBuilder {
        CompleteActionBuilder {
            agent: self.agent,
            tool_spec: None,
        }
    }
}

impl DelegateActionBuilder {
    pub fn and_back(&mut self) -> &mut Self {
        self.and_back = true;
        self
    }

    /// Customize the tool specification for the initial delegate action
    pub fn tool_spec(&mut self, tool_spec: ToolSpec) -> &mut Self {
        self.tool_spec = Some(tool_spec);
        self
    }

    /// Customize the tool specification for delegating back
    pub fn back_tool_spec(&mut self, tool_spec: ToolSpec) -> &mut Self {
        self.back_tool_spec = Some(tool_spec);
        self
    }
}

impl CompleteActionBuilder {
    pub fn tool_spec(&mut self, tool_spec: ToolSpec) -> &mut Self {
        self.tool_spec = Some(tool_spec);
        self
    }
}

impl From<DelegateActionBuilder> for Action {
    fn from(builder: DelegateActionBuilder) -> Self {
        Action::Delegate(DelegateAction {
            from_agent: builder.from_agent,
            to_agent: builder.to_agent,
            and_back: builder.and_back,
            tool_spec: builder.tool_spec,
            back_tool_spec: builder.back_tool_spec,
        })
    }
}

impl From<&mut DelegateActionBuilder> for Action {
    fn from(builder: &mut DelegateActionBuilder) -> Self {
        Action::Delegate(DelegateAction {
            from_agent: builder.from_agent.clone(),
            to_agent: builder.to_agent.clone(),
            and_back: builder.and_back,
            tool_spec: builder.tool_spec.clone(),
            back_tool_spec: builder.back_tool_spec.clone(),
        })
    }
}

impl From<CompleteActionBuilder> for Action {
    fn from(builder: CompleteActionBuilder) -> Self {
        Action::Complete(CompleteAction {
            agent: builder.agent,
            tool_spec: builder.tool_spec,
        })
    }
}

impl From<&mut CompleteActionBuilder> for Action {
    fn from(builder: &mut CompleteActionBuilder) -> Self {
        Action::Complete(CompleteAction {
            agent: builder.agent.clone(),
            tool_spec: builder.tool_spec.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use swiftide_core::chat_completion::ParamSpec;

    use super::*;

    #[test]
    fn test_delegate_action_with_and_back() {
        let planning_agent = "planning_agent";
        let research_agent = "research_agent";

        let action = Action::for_agent(planning_agent)
            .delegates_to(research_agent)
            .and_back()
            .into();

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

        let action = Action::for_agent(jsonspec_agent).can_complete().into();

        match action {
            Action::Complete(ca) => {
                assert_eq!(ca.agent, jsonspec_agent);
            }
            _ => panic!("Expected a CompleteAction"),
        }
    }

    #[test]
    fn test_delegate_action_with_custom_tool_spec() {
        let planning_agent = "planning_agent";
        let research_agent = "research_agent";

        let custom_tool_spec = ToolSpec::builder()
            .name("custom_delegate_tool")
            .description("Custom delegation tool")
            .parameters(vec![
                ParamSpec::builder()
                    .name("custom_param")
                    .description("A custom parameter for delegation")
                    .build()
                    .unwrap(),
            ])
            .build()
            .unwrap();

        let action = Action::for_agent(planning_agent)
            .delegates_to(research_agent)
            .tool_spec(custom_tool_spec.clone())
            .into();

        match action {
            Action::Delegate(da) => {
                assert_eq!(da.from_agent, planning_agent);
                assert_eq!(da.to_agent, research_agent);
                let built_tool_spec = da.tool_spec.unwrap();
                assert_eq!(built_tool_spec.name, "custom_delegate_tool");
                assert_eq!(built_tool_spec.description, "Custom delegation tool");
                assert_eq!(built_tool_spec.parameters.len(), 1);
                assert_eq!(built_tool_spec.parameters[0].name, "custom_param");
                assert_eq!(
                    built_tool_spec.parameters[0].description,
                    "A custom parameter for delegation"
                );
            }
            _ => panic!("Expected a DelegateAction"),
        }
    }

    #[test]
    fn test_complete_action_with_custom_tool_spec() {
        let jsonspec_agent = "jsonspec_agent";

        let custom_tool_spec = ToolSpec::builder()
            .name("custom_complete_tool")
            .description("Custom completion tool")
            .build()
            .unwrap();

        let action = Action::for_agent(jsonspec_agent)
            .can_complete()
            .tool_spec(custom_tool_spec)
            .into();

        match action {
            Action::Complete(ca) => {
                assert_eq!(ca.agent, jsonspec_agent);
                let built_tool_spec = ca.tool_spec.unwrap();
                assert_eq!(built_tool_spec.name, "custom_complete_tool");
                assert_eq!(built_tool_spec.description, "Custom completion tool");
            }
            _ => panic!("Expected a CompleteAction"),
        }
    }
}
