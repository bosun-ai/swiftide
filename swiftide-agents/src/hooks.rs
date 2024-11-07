use anyhow::Result;
use std::{future::Future, pin::Pin};

use dyn_clone::DynClone;
use swiftide_core::AgentContext;

// pub type HookFn = Box<dyn Fn(&mut dyn AgentContext) -> Result<()>>;
//
// dyn_clone::clone_trait_object!(HookFn);
pub trait HookFn:
    for<'a> Fn(&'a mut dyn AgentContext) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
    + Send
    + Sync
    + DynClone
{
}

dyn_clone::clone_trait_object!(HookFn);

#[derive(Clone, strum_macros::EnumDiscriminants)]
#[strum_discriminants(name(HookTypes))]
pub enum Hook {
    BeforeAll(Box<dyn HookFn>),
    BeforeEach(Box<dyn HookFn>),
    AfterTool(Box<dyn HookFn>),
    AfterEach(Box<dyn HookFn>),
    AfterAll(Box<dyn HookFn>),
}

impl<F> HookFn for F where
    F: for<'a> Fn(
            &'a mut dyn AgentContext,
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
            .after_tool(|_| Box::pin(async { Ok(()) }))
            .after_each(|_| Box::pin(async { Ok(()) }))
            .after_all(|_| Box::pin(async { Ok(()) }));
    }
}
