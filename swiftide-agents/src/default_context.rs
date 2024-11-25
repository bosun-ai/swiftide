//! Manages agent history and provides an
//! interface for the external world
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::chat_completion::ChatMessage;
use swiftide_core::{AgentContext, Command, Output, ToolExecutor};
use tokio::sync::Mutex;

use crate::tools::local_executor::LocalExecutor;

// TODO: Remove unit as executor and implement a local executor instead
#[derive(Clone)]
pub struct DefaultContext<EXECUTOR: ToolExecutor = LocalExecutor> {
    completion_history: Arc<Mutex<Vec<ChatMessage>>>,
    should_stop: Arc<AtomicBool>,
    /// Index in the conversation history where the next completion will start
    completions_ptr: Arc<AtomicUsize>,

    /// Index in the conversation history where the current completion started
    /// Allows for retrieving only new messages since the last completion
    current_completions_ptr: Arc<AtomicUsize>,
    tool_executor: EXECUTOR,
}

impl Default for DefaultContext<LocalExecutor> {
    fn default() -> Self {
        DefaultContext {
            completion_history: Arc::new(Mutex::new(Vec::new())),
            should_stop: Arc::new(AtomicBool::new(false)),
            completions_ptr: Arc::new(AtomicUsize::new(0)),
            current_completions_ptr: Arc::new(AtomicUsize::new(0)),
            tool_executor: LocalExecutor::default(),
        }
    }
}

impl<T: ToolExecutor> DefaultContext<T> {
    pub fn from_executor(executor: T) -> DefaultContext<T> {
        DefaultContext {
            tool_executor: executor,
            completion_history: Arc::new(Mutex::new(Vec::new())),
            should_stop: Arc::new(AtomicBool::new(false)),
            completions_ptr: Arc::new(AtomicUsize::new(0)),
            current_completions_ptr: Arc::new(AtomicUsize::new(0)),
        }
    }
}
/// Default, simple implementation of context
///
/// Not meant for concurrent usage.
#[async_trait]
impl<EXECUTOR: ToolExecutor> AgentContext for DefaultContext<EXECUTOR> {
    // TODO: Kinda looks like an iterator now
    async fn next_completion(&self) -> Option<Vec<ChatMessage>> {
        let current = self.completions_ptr.load(Ordering::SeqCst);

        let history = self.completion_history.lock().await;
        let is_last_message_assistant = history.last().is_some_and(ChatMessage::is_assistant);

        if history[current..].is_empty()
            || is_last_message_assistant
            || self.should_stop.load(Ordering::SeqCst)
        {
            None
        } else {
            let previous = self.completions_ptr.swap(history.len(), Ordering::SeqCst);
            self.current_completions_ptr
                .store(previous, Ordering::SeqCst);

            Some(history.clone())
        }
    }

    async fn current_new_messages(&self) -> Vec<ChatMessage> {
        let current = self.current_completions_ptr.load(Ordering::SeqCst);
        let end = self.completions_ptr.load(Ordering::SeqCst);

        let history = self.completion_history.lock().await;

        history[current..end].to_vec()
    }

    async fn history(&self) -> Vec<ChatMessage> {
        self.completion_history.lock().await.clone()
    }

    async fn add_messages(&self, messages: &[ChatMessage]) {
        for item in messages {
            self.add_message(item).await;
        }

        // Debug assert that there is only one ChatMessage::System
        // TODO: Properly handle this
    }

    async fn add_message(&self, item: &ChatMessage) {
        self.completion_history.lock().await.push(item.clone());

        debug_assert!(
            self.completion_history
                .lock()
                .await
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
    use crate::{assistant, tool_output, user};

    use super::*;
    use swiftide_core::chat_completion::{ChatMessage, ToolCall, ToolOutput};

    #[tokio::test]
    async fn test_iteration_tracking() {
        let context = DefaultContext::default();

        // Record initial chat messages
        context
            .add_messages(&[
                ChatMessage::System("You are awesome".into()),
                ChatMessage::User("Hello".into()),
            ])
            .await;

        let messages = context.next_completion().await.unwrap();
        assert_eq!(messages.len(), 2);
        assert!(context.next_completion().await.is_none());

        context
            .add_messages(&[assistant!("Hey?"), user!("How are you?")])
            .await;

        let messages = context.next_completion().await.unwrap();
        assert_eq!(messages.len(), 4);
        assert!(context.next_completion().await.is_none());

        // If the last message is from the assistant, we should not get any more completions
        context.add_messages(&[assistant!("I am fine")]).await;

        assert!(context.next_completion().await.is_none());

        // If there are messages, but the context is stopped, we should not get any more completions
        context
            .add_messages(&[ChatMessage::User("I am fine".into())])
            .await;

        context.stop();

        assert!(context.next_completion().await.is_none());
    }

    #[tokio::test]
    async fn test_should_complete_after_tool_call() {
        let context = DefaultContext::default();
        // Record initial chat messages
        context
            .add_messages(&[
                ChatMessage::System("You are awesome".into()),
                ChatMessage::User("Hello".into()),
            ])
            .await;
        let messages = context.next_completion().await.unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(context.current_new_messages().await.len(), 2);
        assert!(context.next_completion().await.is_none());

        let tool_call = ToolCall::builder().id("1").name("test").build().unwrap();

        context
            .add_messages(&[assistant!("Hey?", ["test"]), tool_output!("test", "Hoi")])
            .await;

        let messages = context.next_completion().await.unwrap();
        assert_eq!(context.current_new_messages().await.len(), 2);
        assert_eq!(messages.len(), 4);

        assert!(context.next_completion().await.is_none());

        // If the last message is from the assistant, we should not get any more completions
        context.add_messages(&[assistant!("I am fine")]).await;
        assert!(context.next_completion().await.is_none());
    }
}
