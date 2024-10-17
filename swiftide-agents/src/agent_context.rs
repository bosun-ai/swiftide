//! Manages agent history and provides an
//! interface for the external world
use crate::traits::{self, Command, CommandOutput};
use anyhow::Result;
use async_trait::async_trait;

#[derive(Clone)]
pub struct AgentContext {}

#[async_trait]
impl traits::AgentContext for AgentContext {
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput> {
        unimplemented!()
    }

    async fn conversation_history(&self) -> &[traits::MessageHistoryRecord] {
        todo!()
    }

    async fn record_message_history(&mut self, history: impl Into<traits::MessageHistoryRecord>) {
        todo!()
    }
}
