//! Manages agent history and provides an interface for the external world
//!
//! This is the default for agents. It is fully async and shareable between agents.
//!
//! By default uses the `LocalExecutor` for tool execution.
//!
//! If chat messages include a `ChatMessage::Summary`, all previous messages are ignored except the
//! system prompt. This is useful for maintaining focus in long conversations or managing token
//! limits.
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::chat_completion::ChatMessage;
use swiftide_core::{AgentContext, Command, CommandError, CommandOutput, ToolExecutor};

use crate::tools::local_executor::LocalExecutor;

// TODO: Remove unit as executor and implement a local executor instead
#[derive(Clone)]
pub struct DefaultContext {
    completion_history: Arc<Mutex<Vec<ChatMessage>>>,
    /// Index in the conversation history where the next completion will start
    completions_ptr: Arc<AtomicUsize>,

    /// Index in the conversation history where the current completion started
    /// Allows for retrieving only new messages since the last completion
    current_completions_ptr: Arc<AtomicUsize>,

    /// The executor used to run tools. I.e. local, remote, docker
    tool_executor: Arc<dyn ToolExecutor>,

    /// Stop if last message is from the assistant
    stop_on_assistant: bool,
}

impl Default for DefaultContext {
    fn default() -> Self {
        DefaultContext {
            completion_history: Arc::new(Mutex::new(Vec::new())),
            completions_ptr: Arc::new(AtomicUsize::new(0)),
            current_completions_ptr: Arc::new(AtomicUsize::new(0)),
            tool_executor: Arc::new(LocalExecutor::default()) as Arc<dyn ToolExecutor>,
            stop_on_assistant: true,
        }
    }
}

impl std::fmt::Debug for DefaultContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefaultContext")
            .field("completion_history", &self.completion_history)
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

    /// Build a context from an existing message history
    ///
    /// # Panics
    ///
    /// Panics if the inner mutex is poisoned
    pub fn with_message_history<I: IntoIterator<Item = ChatMessage>>(
        &mut self,
        message_history: I,
    ) -> &mut Self {
        self.completion_history
            .lock()
            .unwrap()
            .extend(message_history);

        self
    }
}
#[async_trait]
impl AgentContext for DefaultContext {
    /// Retrieve messages for the next completion
    async fn next_completion(&self) -> Option<Vec<ChatMessage>> {
        let history = self.completion_history.lock().unwrap();

        let current = self.completions_ptr.load(Ordering::SeqCst);

        if history[current..].is_empty()
            || (self.stop_on_assistant
                && matches!(history.last(), Some(ChatMessage::Assistant(_, _))))
        {
            None
        } else {
            let previous = self.completions_ptr.swap(history.len(), Ordering::SeqCst);
            self.current_completions_ptr
                .store(previous, Ordering::SeqCst);

            Some(filter_messages_since_summary(history.clone()))
        }
    }

    /// Returns the messages the agent is currently completing on
    async fn current_new_messages(&self) -> Vec<ChatMessage> {
        let current = self.current_completions_ptr.load(Ordering::SeqCst);
        let end = self.completions_ptr.load(Ordering::SeqCst);

        let history = self.completion_history.lock().unwrap();

        filter_messages_since_summary(history[current..end].to_vec())
    }

    /// Retrieve all messages in the conversation history
    async fn history(&self) -> Vec<ChatMessage> {
        self.completion_history.lock().unwrap().clone()
    }

    /// Add multiple messages to the conversation history
    async fn add_messages(&self, messages: Vec<ChatMessage>) {
        for item in messages {
            self.add_message(item).await;
        }
    }

    /// Add a single message to the conversation history
    async fn add_message(&self, item: ChatMessage) {
        self.completion_history.lock().unwrap().push(item);
    }

    /// Execute a command in the tool executor
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError> {
        self.tool_executor.exec_cmd(cmd).await
    }

    /// Pops the last messages up until the previous completion
    ///
    /// LLMs failing completion for various reasons is unfortunately a common occurrence
    /// This gives a way to redrive the last completion in a generic way
    async fn redrive(&self) {
        let mut history = self.completion_history.lock().unwrap();
        let previous = self.current_completions_ptr.load(Ordering::SeqCst);
        let redrive_ptr = self.completions_ptr.swap(previous, Ordering::SeqCst);

        // delete everything after the last completion
        history.truncate(redrive_ptr);
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
            .await;

        let messages = context.next_completion().await.unwrap();
        assert_eq!(messages.len(), 2);
        assert!(context.next_completion().await.is_none());

        context
            .add_messages(vec![assistant!("Hey?"), user!("How are you?")])
            .await;

        let messages = context.next_completion().await.unwrap();
        assert_eq!(messages.len(), 4);
        assert!(context.next_completion().await.is_none());

        // If the last message is from the assistant, we should not get any more completions
        context.add_messages(vec![assistant!("I am fine")]).await;

        assert!(context.next_completion().await.is_none());

        context.with_stop_on_assistant(false);

        assert!(context.next_completion().await.is_some());
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
            .await;
        let messages = context.next_completion().await.unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(context.current_new_messages().await.len(), 2);
        assert!(context.next_completion().await.is_none());

        context
            .add_messages(vec![
                assistant!("Hey?", ["test"]),
                tool_output!("test", "Hoi"),
            ])
            .await;

        let messages = context.next_completion().await.unwrap();
        assert_eq!(context.current_new_messages().await.len(), 2);
        assert_eq!(messages.len(), 4);

        assert!(context.next_completion().await.is_none());
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
        context.add_messages(messages).await;

        let new_messages = context.next_completion().await.unwrap();

        assert_eq!(new_messages.len(), 3);
        assert!(matches!(new_messages[0], ChatMessage::System(_)));
        assert!(matches!(new_messages[1], ChatMessage::Summary(_)));
        assert!(matches!(new_messages[2], ChatMessage::User(_)));

        let current_new_messages = context.current_new_messages().await;
        assert_eq!(current_new_messages.len(), 3);
        assert!(matches!(current_new_messages[0], ChatMessage::System(_)));
        assert!(matches!(current_new_messages[1], ChatMessage::Summary(_)));
        assert!(matches!(current_new_messages[2], ChatMessage::User(_)));

        assert!(context.next_completion().await.is_none());
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
        context.add_messages(messages).await;

        let new_messages = context.next_completion().await.unwrap();

        assert_eq!(new_messages.len(), 3);
        assert!(matches!(new_messages[0], ChatMessage::System(_)));
        assert!(matches!(new_messages[1], ChatMessage::User(_)));
        assert!(matches!(new_messages[2], ChatMessage::Assistant(_, _)));

        context
            .add_message(ChatMessage::Summary("Summary message 1".into()))
            .await;

        let new_messages = context.next_completion().await.unwrap();
        dbg!(&new_messages);
        assert_eq!(new_messages.len(), 2);
        assert!(matches!(new_messages[0], ChatMessage::System(_)));
        assert_eq!(
            new_messages[1],
            ChatMessage::Summary("Summary message 1".into())
        );

        assert!(context.next_completion().await.is_none());

        let messages = vec![
            ChatMessage::User("Hello again".into()),
            ChatMessage::Assistant(Some("Hello there again".into()), None),
        ];

        context.add_messages(messages).await;

        let new_messages = context.next_completion().await.unwrap();

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
            .await;

        let new_messages = context.next_completion().await.unwrap();
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
            .await;

        let messages = context.next_completion().await.unwrap();
        assert_eq!(messages.len(), 2);
        assert!(context.next_completion().await.is_none());
        context.redrive().await;

        let messages = context.next_completion().await.unwrap();
        assert_eq!(messages.len(), 2);

        context
            .add_messages(vec![ChatMessage::User("Hey?".into())])
            .await;

        let messages = context.next_completion().await.unwrap();
        assert_eq!(messages.len(), 3);
        assert!(context.next_completion().await.is_none());
        context.redrive().await;

        // Add more messages
        context
            .add_messages(vec![ChatMessage::User("How are you?".into())])
            .await;

        let messages = context.next_completion().await.unwrap();
        assert_eq!(messages.len(), 4);
        assert!(context.next_completion().await.is_none());

        // Redrive should remove the last set of messages
        dbg!(&context);
        context.redrive().await;
        dbg!(&context);

        // We just redrove with the same messages
        let messages = context.next_completion().await.unwrap();
        assert_eq!(messages.len(), 4);
        assert!(context.next_completion().await.is_none());

        // Add more messages
        context
            .add_messages(vec![
                ChatMessage::User("How are you really?".into()),
                ChatMessage::User("How are you really?".into()),
            ])
            .await;

        // This should remove any additional messages
        context.redrive().await;

        // We just redrove with the same messages
        let messages = context.next_completion().await.unwrap();
        assert_eq!(messages.len(), 4);
        assert!(context.next_completion().await.is_none());

        // Redrive again
        context.redrive().await;
        let messages = context.next_completion().await.unwrap();
        assert_eq!(messages.len(), 4);
        assert!(context.next_completion().await.is_none());
    }
}
