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
//! # use swiftide_agents::Agent;
//! # fn test() {
//! # let mut agent = swiftide_agents::Agent::builder();
//! agent.before_all(move |agent: &Agent| {
//!     Box::pin(async move {
//!         agent.context().add_message(ChatMessage::new_user("Hello, world")).await;
//!         Ok(())
//!     })
//! });
//! # }
//! ```
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
//! # use swiftide_agents::hooks::BeforeAllFn;
//! # use swiftide_agents::Agent;
//! struct SomeHook<'thing> {
//!    thing: &'thing str
//! }
//!
//! impl<'thing> SomeHook<'thing> {
//!    fn return_hook<'tool>(&'thing self) -> impl BeforeAllFn + 'tool where 'thing: 'tool {
//!     move |_: &Agent| {
//!      Box::pin(async move {{ Ok(())}})
//!     }
//!   }
//! }
use anyhow::Result;
use std::{future::Future, pin::Pin};

use dyn_clone::DynClone;
use swiftide_core::chat_completion::{
    errors::ToolError, ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ToolCall,
    ToolOutput,
};

use crate::{errors::AgentError, state::StopReason, Agent};

pub trait BeforeAllFn:
    for<'a> Fn(&'a Agent) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
    + Send
    + Sync
    + DynClone
{
}

dyn_clone::clone_trait_object!(BeforeAllFn);

pub trait AfterEachFn:
    for<'a> Fn(&'a Agent) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
    + Send
    + Sync
    + DynClone
{
}

dyn_clone::clone_trait_object!(AfterEachFn);

pub trait BeforeCompletionFn:
    for<'a> Fn(
        &'a Agent,
        &mut ChatCompletionRequest,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
    + Send
    + Sync
    + DynClone
{
}

dyn_clone::clone_trait_object!(BeforeCompletionFn);

pub trait AfterCompletionFn:
    for<'a> Fn(
        &'a Agent,
        &mut ChatCompletionResponse,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
    + Send
    + Sync
    + DynClone
{
}

dyn_clone::clone_trait_object!(AfterCompletionFn);

/// Hooks that are called after each tool
pub trait AfterToolFn:
    for<'tool> Fn(
        &'tool Agent,
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
    for<'a> Fn(&'a Agent, &ToolCall) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
    + Send
    + Sync
    + DynClone
{
}

dyn_clone::clone_trait_object!(BeforeToolFn);

/// Hooks that are called when a new message is added to the `AgentContext`
pub trait MessageHookFn:
    for<'a> Fn(&'a Agent, &mut ChatMessage) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
    + Send
    + Sync
    + DynClone
{
}

dyn_clone::clone_trait_object!(MessageHookFn);

/// Hooks that are called when the agent starts, either from pending or stopped
pub trait OnStartFn:
    for<'a> Fn(&'a Agent) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
    + Send
    + Sync
    + DynClone
{
}

dyn_clone::clone_trait_object!(OnStartFn);

/// Hooks that are called when the agent stop
pub trait OnStopFn:
    for<'a> Fn(
        &'a Agent,
        StopReason,
        Option<&AgentError>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
    + Send
    + Sync
    + DynClone
{
}

dyn_clone::clone_trait_object!(OnStopFn);

/// Wrapper around the different types of hooks
#[derive(Clone, strum_macros::EnumDiscriminants, strum_macros::Display)]
#[strum_discriminants(name(HookTypes), derive(strum_macros::Display))]
pub enum Hook {
    /// Runs only once for the agent when it starts
    BeforeAll(Box<dyn BeforeAllFn>),
    /// Runs before every completion, yielding a mutable reference to the completion request
    BeforeCompletion(Box<dyn BeforeCompletionFn>),
    /// Runs after every completion, yielding a mutable reference to the completion response
    AfterCompletion(Box<dyn AfterCompletionFn>),
    /// Runs before every tool call, yielding a reference to the tool call
    BeforeTool(Box<dyn BeforeToolFn>),
    /// Runs after every tool call, yielding a reference to the tool call and a mutable result
    AfterTool(Box<dyn AfterToolFn>),
    /// Runs after all tools have completed and a single completion has been made
    AfterEach(Box<dyn AfterEachFn>),
    /// Runs when a new message is added to the `AgentContext`, yielding a mutable reference to the
    /// message. This is only triggered when the message is added by the agent.
    OnNewMessage(Box<dyn MessageHookFn>),
    /// Runs when the agent starts, either from pending or stopped
    OnStart(Box<dyn OnStartFn>),
    /// Runs when the agent stops
    OnStop(Box<dyn OnStopFn>),
}

impl<F> BeforeAllFn for F where
    F: for<'a> Fn(&'a Agent) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
        + Send
        + Sync
        + DynClone
{
}

impl<F> AfterEachFn for F where
    F: for<'a> Fn(&'a Agent) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
        + Send
        + Sync
        + DynClone
{
}

impl<F> BeforeCompletionFn for F where
    F: for<'a> Fn(
            &'a Agent,
            &mut ChatCompletionRequest,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
        + Send
        + Sync
        + DynClone
{
}

impl<F> AfterCompletionFn for F where
    F: for<'a> Fn(
            &'a Agent,
            &mut ChatCompletionResponse,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
        + Send
        + Sync
        + DynClone
{
}

impl<F> BeforeToolFn for F where
    F: for<'a> Fn(&'a Agent, &ToolCall) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
        + Send
        + Sync
        + DynClone
{
}
impl<F> AfterToolFn for F where
    F: for<'tool> Fn(
            &'tool Agent,
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
            &'a Agent,
            &mut ChatMessage,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
        + Send
        + Sync
        + DynClone
{
}

impl<F> OnStartFn for F where
    F: for<'a> Fn(&'a Agent) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
        + Send
        + Sync
        + DynClone
{
}

impl<F> OnStopFn for F where
    F: for<'a> Fn(
            &'a Agent,
            StopReason,
            Option<&AgentError>,
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
            .on_start(|_| Box::pin(async { Ok(()) }))
            .before_completion(|_, _| Box::pin(async { Ok(()) }))
            .before_tool(|_, _| Box::pin(async { Ok(()) }))
            .after_tool(|_, _, _| Box::pin(async { Ok(()) }))
            .after_completion(|_, _| Box::pin(async { Ok(()) }));
    }
}
