//! Manages agent history and provides an
//! interface for the external world
use crate::traits::{self, Command, CommandOutput, ToolCall, ToolOutput, Workspace};
use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::chat_completion::ChatMessage;

#[derive(Clone)]
pub struct AgentContext {
    workspace: Box<dyn Workspace>,
    conversation_history: Vec<MessageHistoryRecord>,
}

impl AgentContext {
    pub async fn workspace(&self) -> &Box<dyn Workspace> {
        &self.workspace
    }

    pub async fn conversation_history(&self) -> &[MessageHistoryRecord] {
        &self.conversation_history
    }

    pub async fn record_in_history(&mut self, item: impl Into<MessageHistoryRecord>) {
        self.conversation_history.push(item.into())
    }

    // Need to think about changing and compressing history, while preserving actual. Tree?
}

#[derive(Clone)]
pub enum MessageHistoryRecord {
    ToolCall(ToolCall),
    ChatMessage(ChatMessage),
    ToolOutput(ToolOutput),
}
