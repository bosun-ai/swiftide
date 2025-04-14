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

use thiserror::Error;

use crate::tasks::delegate_tool::DelegateAgentBuilder;

use super::task::{Task, TaskBuilder};

#[derive(Debug, Clone)]
pub enum Action {
    Delegate(DelegateAction),
    Complete(CompleteAction),
}

#[derive(Debug, Clone)]
pub struct DelegateAction {
    from_agent: String,
    to_agent: String,
    and_back: bool,
}

#[derive(Debug, Clone)]
pub struct CompleteAction {
    agent: String,
}

pub struct ActionBuilder {
    agent: String,
}

pub struct DelegateActionBuilder {
    from_agent: String,
    to_agent: String,
    and_back: bool,
}

#[derive(Debug, Error)]
enum ActionError {
    #[error("Failed to apply action")]
    ApplyFailed,
}

impl Action {
    pub fn for_agent<S: Into<String>>(agent: S) -> ActionBuilder {
        ActionBuilder {
            agent: agent.into(),
        }
    }

    /// Applies this action to the task. Tasks apply all their configured actions
    /// after the build.
    async fn apply(self, task: &mut Task) -> Result<(), ActionError> {
        match self {
            Action::Delegate(delegate_action) => {
                // Build the delegate tool base on the action
                let source = task.find_agent(&delegate_action.from_agent).await.unwrap();
                let target = task.find_agent(&delegate_action.to_agent).await.unwrap();

                let tool = DelegateAgentBuilder::default().delegates_to(target).build()?;

                source.lock().await();
                // Add the delegate tool to the agent
                // If `and_back` is set, also add a delegate tool to the other agent
                todo!()
            }
            Action::Complete(complete_action) => todo!(),
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
