//! Manages agent history and provides an interface for the external world
//!
//! This is the default for agents. It is fully async and shareable between agents.
//!
//! By default uses the `LocalExecutor` for tool execution.
//!
//! If chat messages include a `ChatMessage::Summary`, all previous messages are ignored except the
//! system prompt. This is useful for maintaining focus in long conversations or managing token
//! limits.
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::{
    AgentContext, Command, CommandError, CommandOutput, MessageHistory, ToolExecutor,
};
use swiftide_core::{
    ToolFeedback,
    chat_completion::{ChatMessage, ToolCall},
};

use crate::tools::local_executor::LocalExecutor;

// TODO: Remove unit as executor and implement a local executor instead
#[derive(Clone)]
pub struct DefaultContext {
    /// Responsible for managing the conversation history
    ///
    /// By default, this is a `Arc<Mutex<Vec<ChatMessage>>>`.
    message_history: Arc<dyn MessageHistory>,
    /// Index in the conversation history where the next completion will start
    completions_ptr: Arc<AtomicUsize>,

    /// Index in the conversation history where the current completion started
    /// Allows for retrieving only new messages since the last completion
    current_completions_ptr: Arc<AtomicUsize>,

    /// The executor used to run tools. I.e. local, remote, docker
    tool_executor: Arc<dyn ToolExecutor>,

    /// Stop if last message is from the assistant
    stop_on_assistant: bool,

    feedback_received: Arc<Mutex<HashMap<ToolCall, ToolFeedback>>>,
}

impl Default for DefaultContext {
    fn default() -> Self {
        DefaultContext {
            message_history: Arc::new(Mutex::new(Vec::new())),
            completions_ptr: Arc::new(AtomicUsize::new(0)),
            current_completions_ptr: Arc::new(AtomicUsize::new(0)),
            tool_executor: Arc::new(LocalExecutor::default()) as Arc<dyn ToolExecutor>,
            stop_on_assistant: true,
            feedback_received: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl std::fmt::Debug for DefaultContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefaultContext")
            .field("completion_history", &self.message_history)
            .field("completions_ptr", &self.completions_ptr)
            .field("current_completions_ptr", &self.current_completions_ptr)
            .field("tool_executor", &"Arc<dyn ToolExecutor>")
            .field("stop_on_assistant", &self.stop_on_assistant)
            .finish()
    }
}

impl DefaultContext {
    /// Create a new context with a custom executor
    pub fn from_executor<T: Into<Arc<dyn ToolExecutor>>>(executor: T) -> DefaultContext {
        DefaultContext {
            tool_executor: executor.into(),
            ..Default::default()
        }
    }

    /// If set to true, the agent will stop if the last message is from the assistant (i.e. no new
    /// tool calls, summaries or user messages)
    pub fn with_stop_on_assistant(&mut self, stop: bool) -> &mut Self {
        self.stop_on_assistant = stop;
        self
    }

    pub fn with_agent_backend(&mut self, backend: impl MessageHistory + 'static) -> &mut Self {
        self.message_history = Arc::new(backend) as Arc<dyn MessageHistory>;
        self
    }

    /// Build a context from an existing message history
    ///
    /// # Errors
    ///
    /// Errors if the message history cannot be extended
    ///
    /// # Panics
    ///
    /// Panics if the inner mutex is poisoned
    pub async fn with_message_history<I: IntoIterator<Item = ChatMessage>>(
        &mut self,
        message_history: I,
    ) -> Result<&mut Self> {
        self.message_history
            .extend_owned(message_history.into_iter().collect::<Vec<_>>())
            .await?;

        Ok(self)
    }

    /// Add existing tool feedback to the context
    ///
    /// # Panics
    ///
    /// Panics if the inner mutex is poisoned
    pub fn with_tool_feedback(&mut self, feedback: impl Into<HashMap<ToolCall, ToolFeedback>>) {
        self.feedback_received
            .lock()
            .unwrap()
            .extend(feedback.into());
    }
}
#[async_trait]
impl AgentContext for DefaultContext {
    /// Retrieve messages for the next completion
    async fn next_completion(&self) -> Result<Option<Vec<ChatMessage>>> {
        let history = self.message_history.history().await?;

        let current = self.completions_ptr.load(Ordering::SeqCst);

        if history[current..].is_empty()
            || (self.stop_on_assistant
                && matches!(history.last(), Some(ChatMessage::Assistant(_, _)))
                && self.feedback_received.lock().unwrap().is_empty())
        {
            tracing::debug!(?history, "No new messages for completion");
            Ok(None)
        } else {
            let previous = self.completions_ptr.swap(history.len(), Ordering::SeqCst);
            self.current_completions_ptr
                .store(previous, Ordering::SeqCst);

            Ok(Some(filter_messages_since_summary(history)))
        }
    }

    /// Returns the messages the agent is currently completing on
    async fn current_new_messages(&self) -> Result<Vec<ChatMessage>> {
        let current = self.current_completions_ptr.load(Ordering::SeqCst);
        let end = self.completions_ptr.load(Ordering::SeqCst);

        let history = self.message_history.history().await?;

        Ok(filter_messages_since_summary(
            history[current..end].to_vec(),
        ))
    }

    /// Retrieve all messages in the conversation history
    async fn history(&self) -> Result<Vec<ChatMessage>> {
        self.message_history.history().await
    }

    /// Add multiple messages to the conversation history
    async fn add_messages(&self, messages: Vec<ChatMessage>) -> Result<()> {
        self.message_history.extend_owned(messages).await
    }

    /// Add a single message to the conversation history
    async fn add_message(&self, item: ChatMessage) -> Result<()> {
        self.message_history.push_owned(item).await
    }

    /// Execute a command in the tool executor
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError> {
        self.tool_executor.exec_cmd(cmd).await
    }

    fn executor(&self) -> &Arc<dyn ToolExecutor> {
        &self.tool_executor
    }

    /// Pops the last messages up until the previous completion
    ///
    /// LLMs failing completion for various reasons is unfortunately a common occurrence
    /// This gives a way to redrive the last completion in a generic way
    async fn redrive(&self) -> Result<()> {
        let mut history = self.message_history.history().await?;
        let previous = self.current_completions_ptr.load(Ordering::SeqCst);
        let redrive_ptr = self.completions_ptr.swap(previous, Ordering::SeqCst);

        // delete everything after the last completion
        history.truncate(redrive_ptr);

        Ok(())
    }

    async fn has_received_feedback(&self, tool_call: &ToolCall) -> Option<ToolFeedback> {
        // If feedback is present, return true with the optional payload,
        // and remove it
        // otherwise return false
        let mut lock = self.feedback_received.lock().unwrap();
        lock.remove(tool_call)
    }

    async fn feedback_received(&self, tool_call: &ToolCall, feedback: &ToolFeedback) -> Result<()> {
        let mut lock = self.feedback_received.lock().unwrap();
        // Set the message counter one back so that on a next try, the agent can resume by
        // trying the tool calls first. Only does this if there are no other approvals
        if lock.is_empty() {
            let previous = self.current_completions_ptr.load(Ordering::SeqCst);
            self.completions_ptr.swap(previous, Ordering::SeqCst);
        }
        tracing::debug!(?tool_call, context = ?self, "feedback received");
        lock.insert(tool_call.clone(), feedback.clone());

        Ok(())
    }
}

fn filter_messages_since_summary(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let mut summary_found = false;
    let mut messages = messages
        .into_iter()
        .rev()
        .filter(|m| {
            if summary_found {
                return matches!(m, ChatMessage::System(_));
            }
            if let ChatMessage::Summary(_) = m {
                summary_found = true;
            }
            true
        })
        .collect::<Vec<_>>();

    messages.reverse();

    messages
}

#[cfg(test)]
mod tests {
    use crate::{assistant, tool_output, user};

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
            .await
            .unwrap();

        let messages = context.next_completion().await.unwrap().unwrap();
        assert_eq!(messages.len(), 2);
        assert!(context.next_completion().await.unwrap().is_none());

        context
            .add_messages(vec![assistant!("Hey?"), user!("How are you?")])
            .await
            .unwrap();

        let messages = context.next_completion().await.unwrap().unwrap();
        assert_eq!(messages.len(), 4);
        assert!(context.next_completion().await.unwrap().is_none());

        // If the last message is from the assistant, we should not get any more completions
        context
            .add_messages(vec![assistant!("I am fine")])
            .await
            .unwrap();

        assert!(context.next_completion().await.unwrap().is_none());

        context.with_stop_on_assistant(false);

        assert!(context.next_completion().await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_should_complete_after_tool_call() {
        let context = DefaultContext::default();
        // Record initial chat messages
        context
            .add_messages(vec![
                ChatMessage::System("You are awesome".into()),
                ChatMessage::User("Hello".into()),
            ])
            .await
            .unwrap();
        let messages = context.next_completion().await.unwrap().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(context.current_new_messages().await.unwrap().len(), 2);
        assert!(context.next_completion().await.unwrap().is_none());

        context
            .add_messages(vec![
                assistant!("Hey?", ["test"]),
                tool_output!("test", "Hoi"),
            ])
            .await
            .unwrap();

        let messages = context.next_completion().await.unwrap().unwrap();
        assert_eq!(context.current_new_messages().await.unwrap().len(), 2);
        assert_eq!(messages.len(), 4);

        assert!(context.next_completion().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_filters_messages_before_summary() {
        let messages = vec![
            ChatMessage::System("System message".into()),
            ChatMessage::User("Hello".into()),
            ChatMessage::Assistant(Some("Hello there".into()), None),
            ChatMessage::Summary("Summary message".into()),
            ChatMessage::User("This should be ignored".into()),
        ];
        let context = DefaultContext::default();
        // Record initial chat messages
        context.add_messages(messages).await.unwrap();

        let new_messages = context.next_completion().await.unwrap().unwrap();

        assert_eq!(new_messages.len(), 3);
        assert!(matches!(new_messages[0], ChatMessage::System(_)));
        assert!(matches!(new_messages[1], ChatMessage::Summary(_)));
        assert!(matches!(new_messages[2], ChatMessage::User(_)));

        let current_new_messages = context.current_new_messages().await.unwrap();
        assert_eq!(current_new_messages.len(), 3);
        assert!(matches!(current_new_messages[0], ChatMessage::System(_)));
        assert!(matches!(current_new_messages[1], ChatMessage::Summary(_)));
        assert!(matches!(current_new_messages[2], ChatMessage::User(_)));

        assert!(context.next_completion().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_filters_messages_before_summary_with_assistant_last() {
        let messages = vec![
            ChatMessage::System("System message".into()),
            ChatMessage::User("Hello".into()),
            ChatMessage::Assistant(Some("Hello there".into()), None),
        ];
        let mut context = DefaultContext::default();
        context.with_stop_on_assistant(false);
        // Record initial chat messages
        context.add_messages(messages).await.unwrap();

        let new_messages = context.next_completion().await.unwrap().unwrap();

        assert_eq!(new_messages.len(), 3);
        assert!(matches!(new_messages[0], ChatMessage::System(_)));
        assert!(matches!(new_messages[1], ChatMessage::User(_)));
        assert!(matches!(new_messages[2], ChatMessage::Assistant(_, _)));

        context
            .add_message(ChatMessage::Summary("Summary message 1".into()))
            .await
            .unwrap();

        let new_messages = context.next_completion().await.unwrap().unwrap();
        dbg!(&new_messages);
        assert_eq!(new_messages.len(), 2);
        assert!(matches!(new_messages[0], ChatMessage::System(_)));
        assert_eq!(
            new_messages[1],
            ChatMessage::Summary("Summary message 1".into())
        );

        assert!(context.next_completion().await.unwrap().is_none());

        let messages = vec![
            ChatMessage::User("Hello again".into()),
            ChatMessage::Assistant(Some("Hello there again".into()), None),
        ];

        context.add_messages(messages).await.unwrap();

        let new_messages = context.next_completion().await.unwrap().unwrap();

        assert!(matches!(new_messages[0], ChatMessage::System(_)));
        assert_eq!(
            new_messages[1],
            ChatMessage::Summary("Summary message 1".into())
        );
        assert_eq!(new_messages[2], ChatMessage::User("Hello again".into()));
        assert_eq!(
            new_messages[3],
            ChatMessage::Assistant(Some("Hello there again".to_string()), None)
        );

        context
            .add_message(ChatMessage::Summary("Summary message 2".into()))
            .await
            .unwrap();

        let new_messages = context.next_completion().await.unwrap().unwrap();
        assert_eq!(new_messages.len(), 2);

        assert!(matches!(new_messages[0], ChatMessage::System(_)));
        assert_eq!(
            new_messages[1],
            ChatMessage::Summary("Summary message 2".into())
        );
    }

    #[tokio::test]
    async fn test_redrive() {
        let context = DefaultContext::default();

        // Record initial chat messages
        context
            .add_messages(vec![
                ChatMessage::System("System message".into()),
                ChatMessage::User("Hello".into()),
            ])
            .await
            .unwrap();

        let messages = context.next_completion().await.unwrap().unwrap();
        assert_eq!(messages.len(), 2);
        assert!(context.next_completion().await.unwrap().is_none());
        context.redrive().await.unwrap();

        let messages = context.next_completion().await.unwrap().unwrap();
        assert_eq!(messages.len(), 2);

        context
            .add_messages(vec![ChatMessage::User("Hey?".into())])
            .await
            .unwrap();

        let messages = context.next_completion().await.unwrap().unwrap();
        assert_eq!(messages.len(), 3);
        assert!(context.next_completion().await.unwrap().is_none());
        context.redrive().await.unwrap();

        // Add more messages
        context
            .add_messages(vec![ChatMessage::User("How are you?".into())])
            .await
            .unwrap();

        let messages = context.next_completion().await.unwrap().unwrap();
        assert_eq!(messages.len(), 4);
        assert!(context.next_completion().await.unwrap().is_none());

        // Redrive should remove the last set of messages
        dbg!(&context);
        context.redrive().await.unwrap();
        dbg!(&context);

        // We just redrove with the same messages
        let messages = context.next_completion().await.unwrap().unwrap();
        assert_eq!(messages.len(), 4);
        assert!(context.next_completion().await.unwrap().is_none());

        // Add more messages
        context
            .add_messages(vec![
                ChatMessage::User("How are you really?".into()),
                ChatMessage::User("How are you really?".into()),
            ])
            .await
            .unwrap();

        // This should remove any additional messages
        context.redrive().await.unwrap();

        // We just redrove with the same messages
        let messages = context.next_completion().await.unwrap().unwrap();
        assert_eq!(messages.len(), 4);
        assert!(context.next_completion().await.unwrap().is_none());

        // Redrive again
        context.redrive().await.unwrap();
        let messages = context.next_completion().await.unwrap().unwrap();
        assert_eq!(messages.len(), 4);
        assert!(context.next_completion().await.unwrap().is_none());
    }
}
