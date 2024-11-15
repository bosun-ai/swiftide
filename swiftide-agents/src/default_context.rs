//! Manages agent history and provides an
//! interface for the external world
use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::chat_completion::ChatMessage;
use swiftide_core::{AgentContext, Command, Output, ToolExecutor};

use crate::tools::local_executor::LocalExecutor;

// TODO: Remove unit as executor and implement a local executor instead
#[derive(Clone)]
pub struct DefaultContext<EXECUTOR: ToolExecutor = LocalExecutor> {
    conversation_history: Vec<ChatMessage>,
    should_stop: bool,
    iterations: usize,
    iteration_ptr: usize,
    this_iteration_ptr: usize,
    tool_executor: EXECUTOR,
}

impl Default for DefaultContext<LocalExecutor> {
    fn default() -> Self {
        DefaultContext {
            conversation_history: Vec::new(),
            should_stop: false,
            iterations: 0,
            iteration_ptr: 0,
            this_iteration_ptr: 0,
            tool_executor: LocalExecutor::default(),
        }
    }
}

/// Default, simple implementation of context
///
/// Not meant for concurrent usage.
#[async_trait]
impl<EXECUTOR: ToolExecutor> AgentContext for DefaultContext<EXECUTOR> {
    async fn completion_history(&self) -> &[ChatMessage] {
        &self.conversation_history
    }

    async fn add_message(&mut self, item: ChatMessage) {
        self.this_iteration_ptr += 1;

        self.conversation_history.push(item);

        // Debug assert that there is only one ChatMessage::System
        // TODO: Properly handle this
        debug_assert!(
            self.conversation_history
                .iter()
                .filter(|msg| msg.is_system())
                .count()
                <= 1
        );
    }

    /// Records the current iteration
    ///
    /// Keeps a pointer of where the current iteration starts
    async fn record_iteration(&mut self) {
        self.iterations += 1;
        self.iteration_ptr += self.this_iteration_ptr;
        self.this_iteration_ptr = 0;
    }

    async fn current_chat_messages(&self) -> &[ChatMessage] {
        &self.conversation_history[self.iteration_ptr..]
    }

    fn should_stop(&self) -> bool {
        self.should_stop
    }

    fn stop(&mut self) {
        self.should_stop = true;
    }

    async fn exec_cmd(&self, cmd: &Command) -> Result<Output> {
        self.tool_executor.exec_cmd(cmd).await
    }
}

impl<T: ToolExecutor> DefaultContext<T> {
    pub fn from_executor(executor: T) -> DefaultContext<T> {
        DefaultContext {
            tool_executor: executor,
            conversation_history: Vec::new(),
            should_stop: false,
            iterations: 0,
            iteration_ptr: 0,
            this_iteration_ptr: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swiftide_core::chat_completion::ChatMessage;

    #[tokio::test]
    async fn test_iteration_tracking() {
        let mut context = DefaultContext::default();

        // Record initial chat messages
        context.add_message(ChatMessage::User("Hello".into())).await;
        context
            .add_message(ChatMessage::Assistant("Hi there!".into()))
            .await;

        assert_eq!(context.current_chat_messages().await.len(), 2);

        // Record the first iteration
        context.record_iteration().await;

        assert_eq!(context.current_chat_messages().await.len(), 0);

        // Record more chat messages
        context
            .add_message(ChatMessage::User("How are you?".into()))
            .await;
        context
            .add_message(ChatMessage::Assistant("I'm good, thanks!".into()))
            .await;

        let current_messages = context.current_chat_messages().await;
        assert_eq!(current_messages.len(), 2);

        assert_eq!(
            current_messages[0],
            ChatMessage::User("How are you?".to_string())
        );
        assert_eq!(
            current_messages[1],
            ChatMessage::Assistant("I'm good, thanks!".to_string())
        );
        // Record the second iteration
        context.record_iteration().await;
    }
}
