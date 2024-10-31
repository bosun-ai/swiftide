use anyhow::Result;
use std::{future::Future, pin::Pin};

use crate::AgentContext;
use dyn_clone::DynClone;

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
