//! Manages agent history and provides an
//! interface for the external world
use crate::traits::AgentContext;
use async_trait::async_trait;
use swiftide_core::chat_completion::ChatMessage;

#[derive(Clone, Default)]
pub struct DefaultContext {
    // workspace: Box<dyn Workspace>,
    conversation_history: Vec<ChatMessage>,
}

/// Default, simple implementation of context
///
/// Not meant for concurrent usage.
#[async_trait]
impl AgentContext for DefaultContext {
    // pub async fn workspace(&self) -> &Box<dyn Workspace> {
    //     &self.workspace
    // }

    async fn conversation_history(&self) -> &[ChatMessage] {
        &self.conversation_history
    }

    async fn record_in_history(&mut self, item: ChatMessage) {
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

    // Need to think about changing and compressing history, while preserving actual. Tree?
}
