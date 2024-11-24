use anyhow::Result;
use std::{future::Future, pin::Pin};

use dyn_clone::DynClone;
use swiftide_core::{
    chat_completion::{errors::ToolError, ToolCall, ToolOutput},
    AgentContext,
};

// pub type HookFn = Box<dyn Fn(&mut dyn AgentContext) -> Result<()>>;
//
// dyn_clone::clone_trait_object!(HookFn);
pub trait HookFn:
    for<'a> Fn(&'a dyn AgentContext) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
    + Send
    + Sync
    + DynClone
{
}

dyn_clone::clone_trait_object!(HookFn);

pub trait ToolHookFn:
    for<'a> Fn(
        &'a dyn AgentContext,
        &ToolCall,
        &mut Result<ToolOutput, ToolError>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
    + Send
    + Sync
    + DynClone
{
}

dyn_clone::clone_trait_object!(ToolHookFn);

#[derive(Clone, strum_macros::EnumDiscriminants, strum_macros::Display)]
#[strum_discriminants(name(HookTypes), derive(strum_macros::Display))]
pub enum Hook {
    BeforeAll(Box<dyn HookFn>),
    BeforeEach(Box<dyn HookFn>),
    AfterTool(Box<dyn ToolHookFn>),
    AfterEach(Box<dyn HookFn>),
    // AfterAll(Box<dyn HookFn>),
}

impl<F> HookFn for F where
    F: for<'a> Fn(&'a dyn AgentContext) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
        + Send
        + Sync
        + DynClone
{
}

impl<F> ToolHookFn for F where
    F: for<'a> Fn(
            &'a dyn AgentContext,
            &ToolCall,
            &mut Result<ToolOutput, ToolError>,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
        + Send
        + Sync
        + DynClone
{
}

#[cfg(test)]
mod tests {
    use crate::Agent;

    #[test]
    fn test_hooks_compile_sync_and_async() {
        // TODO: How to strip the Box::Pin?
        Agent::builder()
            .before_all(|_| Box::pin(async { Ok(()) }))
            .before_each(|_| Box::pin(async { Ok(()) }))
            .after_tool(|_, _, _| Box::pin(async { Ok(()) }))
            .after_each(|_| Box::pin(async { Ok(()) }));
    }
}
