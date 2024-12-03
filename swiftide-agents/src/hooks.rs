//! Hooks are functions that are called at specific points in the agent lifecycle.
//!
//!
//! Since rust does not have async closures, hooks have to return a boxed, pinned async block
//! themselves.
//!
//! # Example
//!
//! ```no_run
//! # use swiftide_core::{AgentContext, chat_completion::ChatMessage};
//! # fn test() {
//! # let mut agent = swiftide_agents::Agent::builder();
//! agent.before_all(move |context: &dyn AgentContext| {
//!     Box::pin(async move {
//!         context.add_message(&ChatMessage::new_user("Hello, world")).await;
//!         Ok(())
//!     })
//!});
//!# }
//!```
//! Rust has a long outstanding issue where it captures outer lifetimes when returning an impl
//! that also has lifetimes, see [this issue](https://github.com/rust-lang/rust/issues/42940)
//!
//! This can happen if you write a method like `fn return_hook(&self) -> impl HookFn`, where the
//! owner also has a lifetime.
//! The trick is to set an explicit lifetime on self, and hook, where self must outlive the hook.
//!
//! # Example
//!
//! ```no_run
//! # use swiftide_core::{AgentContext};
//! # use swiftide_agents::hooks::HookFn;
//! struct SomeHook<'thing> {
//!    thing: &'thing str
//! }
//!
//! impl<'thing> SomeHook<'thing> {
//!    fn return_hook<'tool>(&'thing self) -> impl HookFn + 'tool where 'thing: 'tool {
//!     move |_: &dyn AgentContext| {
//!      Box::pin(async move {{ Ok(())}})
//!     }
//!   }
//!}
use anyhow::Result;
use std::{future::Future, pin::Pin};

use dyn_clone::DynClone;
use swiftide_core::{
    chat_completion::{errors::ToolError, ChatMessage, ToolCall, ToolOutput},
    AgentContext,
};

/// Hooks that are call on before each, after each and before all
pub trait HookFn:
    for<'a> Fn(&'a dyn AgentContext) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
    + Send
    + Sync
    + DynClone
{
}

dyn_clone::clone_trait_object!(HookFn);

/// Hooks that are called after each tool
///
pub trait AfterToolFn:
    for<'tool> Fn(
        &'tool dyn AgentContext,
        &ToolCall,
        &'tool mut Result<ToolOutput, ToolError>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'tool>>
    + Send
    + Sync
    + DynClone
{
}

dyn_clone::clone_trait_object!(AfterToolFn);

/// Hooks that are called before each tool
pub trait BeforeToolFn:
    for<'a> Fn(
        &'a dyn AgentContext,
        &ToolCall,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
    + Send
    + Sync
    + DynClone
{
}

dyn_clone::clone_trait_object!(BeforeToolFn);

/// Hooks that are called when a new message is added to the `AgentContext`
pub trait MessageHookFn:
    for<'a> Fn(
        &'a dyn AgentContext,
        &mut ChatMessage,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
    + Send
    + Sync
    + DynClone
{
}

dyn_clone::clone_trait_object!(MessageHookFn);

/// Wrapper around the different types of hooks
#[derive(Clone, strum_macros::EnumDiscriminants, strum_macros::Display)]
#[strum_discriminants(name(HookTypes), derive(strum_macros::Display))]
pub enum Hook {
    BeforeAll(Box<dyn HookFn>),
    BeforeEach(Box<dyn HookFn>),
    BeforeTool(Box<dyn BeforeToolFn>),
    AfterTool(Box<dyn AfterToolFn>),
    OnNewMessage(Box<dyn MessageHookFn>),
    AfterEach(Box<dyn HookFn>),
}

impl<F> HookFn for F where
    F: for<'a> Fn(&'a dyn AgentContext) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
        + Send
        + Sync
        + DynClone
{
}

impl<F> BeforeToolFn for F where
    F: for<'a> Fn(
            &'a dyn AgentContext,
            &ToolCall,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
        + Send
        + Sync
        + DynClone
{
}
impl<F> AfterToolFn for F where
    F: for<'tool> Fn(
            &'tool dyn AgentContext,
            &ToolCall,
            &'tool mut Result<ToolOutput, ToolError>,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'tool>>
        + Send
        + Sync
        + DynClone
{
}

impl<F> MessageHookFn for F where
    F: for<'a> Fn(
            &'a dyn AgentContext,
            &mut ChatMessage,
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
        Agent::builder()
            .before_all(|_| Box::pin(async { Ok(()) }))
            .before_each(|_| Box::pin(async { Ok(()) }))
            .before_tool(|_, _| Box::pin(async { Ok(()) }))
            .after_tool(|_, _, _| Box::pin(async { Ok(()) }))
            .after_each(|_| Box::pin(async { Ok(()) }));
    }
}
