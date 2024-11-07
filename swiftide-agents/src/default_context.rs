//! Manages agent history and provides an
//! interface for the external world
use async_trait::async_trait;
use swiftide_core::chat_completion::ChatMessage;
use swiftide_core::AgentContext;

#[derive(Clone, Default)]
pub struct DefaultContext {
    // workspace: Box<dyn Workspace>,
    conversation_history: Vec<ChatMessage>,
    should_stop: bool,
    iterations: usize,
    iteration_ptr: usize,
    this_iteration_ptr: usize,
}

/// Default, simple implementation of context
///
/// Not meant for concurrent usage.
#[async_trait]
impl AgentContext for DefaultContext {
    // pub async fn workspace(&self) -> &Box<dyn Workspace> {
    //     &self.workspace
    // }

    async fn completion_history(&self) -> &[ChatMessage] {
        &self.conversation_history
    }

    async fn record_in_history(&mut self, item: ChatMessage) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use swiftide_core::chat_completion::ChatMessage;

    #[tokio::test]
    async fn test_iteration_tracking() {
        let mut context = DefaultContext::default();

        // Record initial chat messages
        context
            .record_in_history(ChatMessage::User("Hello".into()))
            .await;
        context
            .record_in_history(ChatMessage::Assistant("Hi there!".into()))
            .await;

        assert_eq!(context.current_chat_messages().await.len(), 2);

        // Record the first iteration
        context.record_iteration().await;

        assert_eq!(context.current_chat_messages().await.len(), 0);

        // Record more chat messages
        context
            .record_in_history(ChatMessage::User("How are you?".into()))
            .await;
        context
            .record_in_history(ChatMessage::Assistant("I'm good, thanks!".into()))
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
