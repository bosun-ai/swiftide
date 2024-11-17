//! Manages agent history and provides an
//! interface for the external world
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::chat_completion::ChatMessage;
use swiftide_core::{AgentContext, Command, Output, ToolExecutor};

use crate::tools::local_executor::LocalExecutor;

// TODO: Remove unit as executor and implement a local executor instead
#[derive(Clone)]
pub struct DefaultContext<EXECUTOR: ToolExecutor = LocalExecutor> {
    completion_history: Vec<ChatMessage>,
    should_stop: Arc<AtomicBool>,
    /// Index in the conversation history where the next completion will start
    completions_ptr: Arc<AtomicUsize>,
    tool_executor: EXECUTOR,
}

impl Default for DefaultContext<LocalExecutor> {
    fn default() -> Self {
        DefaultContext {
            completion_history: Vec::new(),
            should_stop: Arc::new(AtomicBool::new(false)),
            completions_ptr: Arc::new(AtomicUsize::new(0)),
            tool_executor: LocalExecutor::default(),
        }
    }
}

impl<T: ToolExecutor> DefaultContext<T> {
    pub fn from_executor(executor: T) -> DefaultContext<T> {
        DefaultContext {
            tool_executor: executor,
            completion_history: Vec::new(),
            should_stop: Arc::new(AtomicBool::new(false)),
            completions_ptr: Arc::new(AtomicUsize::new(0)),
        }
    }
}
/// Default, simple implementation of context
///
/// Not meant for concurrent usage.
#[async_trait]
impl<EXECUTOR: ToolExecutor> AgentContext for DefaultContext<EXECUTOR> {
    // TODO: Kinda looks like an iterator now
    async fn next_completion(&self) -> Option<&[ChatMessage]> {
        let current = self.completions_ptr.load(Ordering::SeqCst);

        let history = &self.completion_history;
        let is_last_message_assistant = history.last().is_some_and(ChatMessage::is_assistant);

        if history[current..].is_empty()
            || is_last_message_assistant
            || self.should_stop.load(Ordering::SeqCst)
        {
            None
        } else {
            self.completions_ptr.store(history.len(), Ordering::SeqCst);
            Some(history)
        }
    }

    async fn add_messages(&mut self, messages: Vec<ChatMessage>) {
        for item in messages {
            self.completion_history.push(item);
        }

        // Debug assert that there is only one ChatMessage::System
        // TODO: Properly handle this
        debug_assert!(
            self.completion_history
                .iter()
                .filter(|msg| msg.is_system())
                .count()
                <= 1
        );
    }

    fn stop(&self) {
        self.should_stop.store(true, Ordering::SeqCst);
    }

    async fn exec_cmd(&self, cmd: &Command) -> Result<Output> {
        self.tool_executor.exec_cmd(cmd).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swiftide_core::chat_completion::{ChatMessage, ToolCall};

    #[tokio::test]
    async fn test_iteration_tracking() {
        let mut context = DefaultContext::default();

        // Record initial chat messages
        context
            .add_messages(vec![
                ChatMessage::System("You are awesome".into()),
                ChatMessage::User("Hello".into()),
            ])
            .await;

        let messages = context.next_completion().await.unwrap();
        assert_eq!(messages.len(), 2);
        assert!(context.next_completion().await.is_none());

        context
            .add_messages(vec![
                ChatMessage::Assistant("Hey?".into()),
                ChatMessage::User("How are you?".into()),
            ])
            .await;

        let messages = context.next_completion().await.unwrap();
        assert_eq!(messages.len(), 4);
        assert!(context.next_completion().await.is_none());

        // If the last message is from the assistant, we should not get any more completions
        context
            .add_messages(vec![ChatMessage::Assistant("I am fine".into())])
            .await;

        assert!(context.next_completion().await.is_none());

        // If there are messages, but the context is stopped, we should not get any more completions
        context
            .add_messages(vec![ChatMessage::User("I am fine".into())])
            .await;

        context.stop();

        assert!(context.next_completion().await.is_none());
    }

    #[tokio::test]
    async fn test_should_complete_after_tool_call() {
        let mut context = DefaultContext::default();
        // Record initial chat messages
        context
            .add_messages(vec![
                ChatMessage::System("You are awesome".into()),
                ChatMessage::User("Hello".into()),
            ])
            .await;
        let messages = context.next_completion().await.unwrap();
        assert_eq!(messages.len(), 2);
        assert!(context.next_completion().await.is_none());

        let tool_call = ToolCall::builder().id("1").name("test").build().unwrap();

        context
            .add_messages(vec![
                ChatMessage::Assistant("Hey?".into()),
                ChatMessage::ToolOutput(tool_call, "Hoi".into()),
            ])
            .await;

        let messages = context.next_completion().await.unwrap();
        assert_eq!(messages.len(), 4);

        assert!(context.next_completion().await.is_none());

        // If the last message is from the assistant, we should not get any more completions
        context
            .add_messages(vec![ChatMessage::Assistant("I am fine".into())])
            .await;
        assert!(context.next_completion().await.is_none());
    }
}
