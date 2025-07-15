//! A step represents a single operation in a task
use std::pin::Pin;

use crate::Agent;

use super::{Task, TaskError, TaskState, backend::Backend, running_agent::RunningAgent};

pub type StepFn<B, S> =
    for<'a> fn(&'a Task<B, S>) -> Pin<Box<dyn Future<Output = Result<(), TaskError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub enum Step<B: Backend, S: TaskState> {
    Agent(RunningAgent),
    StepFn(StepFn<B, S>),
    // ForEach(Box<Step>)
}

impl<B: Backend, S: TaskState> Step<B, S> {
    pub fn as_agent(&self) -> Option<&RunningAgent> {
        if let Step::Agent(agent) = self {
            Some(agent)
        } else {
            None
        }
    }

    pub fn as_step_fn(&self) -> Option<&StepFn<B, S>> {
        if let Step::StepFn(step_fn) = self {
            Some(step_fn)
        } else {
            None
        }
    }
}
