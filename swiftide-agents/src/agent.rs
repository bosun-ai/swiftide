#![allow(dead_code)]
use crate::{
    default_context::DefaultContext,
    hooks::{Hook, HookFn, HookTypes},
    state,
    tools::control::Stop,
};
use std::collections::HashSet;

use anyhow::Result;
use derive_builder::Builder;
use dyn_clone::DynClone;
use swiftide_core::{
    chat_completion::{ChatCompletion, ChatCompletionRequest, ChatMessage, ToolOutput},
    AgentContext, Tool,
};
use tracing::debug;

// TODO:
// - [ ] After calling run or run once cannot call run again
// - [ ] Cannot call continue if agent has not called run (state machine?)
//       ... Or should we simplify it, and allow it for now?
// - [ ] Continue is what should happen
// - [ ] Agent should support a system prompt
// - [ ] Hooks should  called at each correct point
// - [ ] Errors should all be thiserror and not anyhow
// - [ ] Improve tracing and logging (need to check when running it)
// - [ ] Consider making tools generic over context instead
// - [ ] Ensure hooks can take both regular functions _and_ closures

// Notes
//
// Generic over LLM instead of box dyn? Should tool support be a separate trait?
#[derive(Clone, Builder)]
pub struct Agent<CONTEXT: AgentContext = DefaultContext> {
    #[builder(default, setter(into))]
    pub(crate) hooks: Vec<Hook>,
    // name: String,
    #[builder(setter(custom))]
    pub(crate) context: CONTEXT,
    #[builder(default = Agent::< CONTEXT>::default_tools(), setter(custom))]
    pub(crate) tools: HashSet<Box<dyn Tool>>,

    #[builder(setter(custom))]
    pub(crate) llm: Box<dyn ChatCompletion>,

    #[builder(private, default = state::State::default())]
    pub(crate) state: state::State,
}

impl<CONTEXT: AgentContext> AgentBuilder<CONTEXT> {
    pub fn context<C: AgentContext>(&mut self, context: C) -> AgentBuilder<C>
    where
        Self: Clone,
    {
        let AgentBuilder {
            hooks,
            tools,
            llm,
            state,
            ..
        } = self.clone();

        // Rust is silly that you can't just forward self without context
        AgentBuilder::<C> {
            context: Some(context),
            hooks,
            tools,
            llm,
            state,
        }
    }

    pub fn add_hook(&mut self, hook: Hook) -> &mut Self {
        let hooks = self.hooks.get_or_insert_with(Vec::new);
        hooks.push(hook);

        self
    }

    pub fn before_all(&mut self, hook: impl HookFn + 'static) -> &mut Self {
        self.add_hook(Hook::BeforeAll(Box::new(hook)))
    }

    pub fn before_each(&mut self, hook: impl HookFn + 'static) -> &mut Self {
        self.add_hook(Hook::BeforeEach(Box::new(hook)))
    }

    pub fn after_tool(&mut self, hook: impl HookFn + 'static) -> &mut Self {
        self.add_hook(Hook::AfterTool(Box::new(hook)))
    }

    pub fn after_each(&mut self, hook: impl HookFn + 'static) -> &mut Self {
        self.add_hook(Hook::AfterEach(Box::new(hook)))
    }

    pub fn after_all(&mut self, hook: impl HookFn + 'static) -> &mut Self {
        self.add_hook(Hook::AfterAll(Box::new(hook)))
    }

    pub fn llm<LLM: ChatCompletion + Clone + 'static>(&mut self, llm: &LLM) -> &mut Self {
        let boxed: Box<dyn ChatCompletion> = Box::new(llm.clone());

        self.llm = Some(boxed);
        self
    }

    pub fn tools<TOOL: Into<Box<dyn Tool>>, I: IntoIterator<Item = TOOL>>(
        &mut self,
        tools: I,
    ) -> &mut Self {
        self.tools = Some(
            tools
                .into_iter()
                .map(Into::into)
                .chain(Agent::<CONTEXT>::default_tools())
                .collect(),
        );
        self
    }
}

impl Agent<DefaultContext> {
    pub fn builder() -> AgentBuilder<DefaultContext> {
        let context = DefaultContext::default();
        AgentBuilder::<DefaultContext>::default()
            .context(context)
            .clone()
    }
}

impl<CONTEXT: AgentContext> Agent<CONTEXT> {
    fn default_tools() -> HashSet<Box<dyn Tool>> {
        HashSet::from([Box::new(Stop::default()) as Box<dyn Tool>])
    }

    pub async fn query(&mut self, query: impl Into<String>) -> Result<()> {
        self.run_agent(Some(query.into()), false).await
    }

    pub async fn query_once(&mut self, query: impl Into<String>) -> Result<()> {
        self.run_agent(Some(query.into()), true).await
    }

    async fn run_agent(&mut self, maybe_query: Option<String>, just_once: bool) -> Result<()> {
        if self.state.is_running() {
            anyhow::bail!("Agent is already running");
        }

        if let Some(query) = maybe_query {
            self.context.add_messages(&[ChatMessage::User(query)]).await;
        }

        if self.state.is_pending() {
            self.invoke_hooks_matching(HookTypes::BeforeAll).await?;
        }

        while let Some(messages) = self.context.next_completion().await {
            self.state = state::State::Running;

            let new_messages = match self.run_completions(&messages).await {
                Ok(messages) => messages,
                Err(e) => {
                    self.state = state::State::Stopped;
                    return Err(e);
                }
            };

            self.context.add_messages(&new_messages).await;
            if just_once {
                break;
            }
        }

        self.state = state::State::Stopped;

        Ok(())
    }

    async fn run_completions(&mut self, messages: &[ChatMessage]) -> Result<Vec<ChatMessage>> {
        self.invoke_hooks_matching(HookTypes::BeforeEach).await?;

        debug!(
            "Running completion for agent with {} messages",
            messages.len()
        );

        let chat_completion_request = ChatCompletionRequest::builder()
            .messages(messages)
            .tools_spec(
                self.tools
                    .iter()
                    .map(|tool| tool.json_spec())
                    .collect::<HashSet<_>>(),
            )
            .build()?;

        debug!("Calling LLM with request: {:?}", chat_completion_request);
        let response = self.llm.complete(&chat_completion_request).await?;

        let mut new_messages = vec![];
        if let Some(message) = response.message {
            debug!("LLM returned message: {}", message);

            new_messages.push(ChatMessage::Assistant(message));
        }

        // TODO: We can and should run tools in parallel or at least in a tokio spawn
        if let Some(tool_calls) = response.tool_calls {
            debug!("LLM returned tool calls: {:?}", tool_calls);
            for tool_call in tool_calls {
                let Some(tool) = self.find_tool_by_name(tool_call.name()) else {
                    tracing::warn!("Tool {} not found", tool_call.name());
                    continue;
                };
                tracing::debug!("Calling tool: {}", tool_call.name());
                let output = tool.invoke(&self.context, tool_call.args()).await?;

                self.handle_control_tools(&output);

                new_messages.push(ChatMessage::ToolOutput(tool_call, output));
            }
        };

        self.invoke_hooks_matching(HookTypes::AfterEach).await?;

        Ok(new_messages)
    }

    async fn invoke_hooks_matching(&mut self, hook_type: HookTypes) -> Result<()> {
        for hook in self.hooks.iter().filter(|h| hook_type == (*h).into()) {
            match hook {
                Hook::BeforeAll(hook) => hook(&mut self.context).await?,
                Hook::BeforeEach(hook) => hook(&mut self.context).await?,
                Hook::AfterTool(hook) => hook(&mut self.context).await?,
                Hook::AfterEach(hook) => hook(&mut self.context).await?,
                // Is this even possible without a definition of done and always being able to
                Hook::AfterAll(hook) => hook(&mut self.context).await?,
                // continue?
            }
        }

        Ok(())
    }

    fn find_tool_by_name(&self, tool_name: &str) -> Option<&dyn Tool> {
        self.tools
            .iter()
            .find(|tool| tool.name() == tool_name)
            .map(|boxed| &**boxed)
    }

    // Handle any tool specific output (e.g. stop)
    fn handle_control_tools(&self, output: &ToolOutput) {
        if let ToolOutput::Stop = output {
            self.context.stop();
        }
    }
}

// pub async fn history(&self) -> &[ChatMessage] {
//     self.context.completion_history().await
// }

/// Runs the agent
///
/// # Errors
///
/// Any error that occurs during the agent's execution is returned.
// pub async fn run(&mut self) -> Result<()> {
//     debug!("Running agent");
//     self.context
//         .add_message(ChatMessage::User(self.instructions.render().await?))
//         .await;
//
//     self.invoke_hooks_matching(HookTypes::BeforeAll).await?;
//
//     while !self.context.should_stop() {
//         debug!("Looping agent");
//
//         self.invoke_hooks_matching(HookTypes::BeforeEach).await?;
//         self.run_once().await?;
//
//         self.invoke_hooks_matching(HookTypes::AfterEach).await?;
//
//         if self.context.current_chat_messages().await.is_empty() {
//             warn!("No new messages for LLM, stopping agent");
//             self.context.stop();
//         }
//     }
//
//     self.invoke_hooks_matching(HookTypes::AfterAll).await?;
//     Ok(())
// }

#[cfg(test)]
mod tests {

    use swiftide_core::chat_completion::{ChatCompletionResponse, ToolCall};
    use swiftide_core::test_utils::MockChatCompletion;

    use super::*;
    use crate::{assistant, chat_request, chat_response, tool_output, user};

    use crate::test_utils::{MockHook, MockTool};

    #[test_log::test(tokio::test)]
    async fn test_agent_builder_defaults() {
        // Create a prompt
        let mock_llm = MockChatCompletion::new();

        // Build the agent
        let agent = Agent::builder().llm(&mock_llm).build().unwrap();

        // Check that the context is the default context

        // Check that the default tools are added
        assert!(agent.find_tool_by_name("stop").is_some());

        // Check it does not allow duplicates
        let agent = Agent::builder()
            .tools([Stop::default(), Stop::default()])
            .llm(&mock_llm)
            .build()
            .unwrap();

        assert_eq!(agent.tools.len(), 1);

        // It should include the default tool if a different tool is provided
        let agent = Agent::builder()
            .tools([MockTool::new()])
            .llm(&mock_llm)
            .build()
            .unwrap();

        assert_eq!(agent.tools.len(), 2);
        assert!(agent.find_tool_by_name("mock_tool").is_some());
        assert!(agent.find_tool_by_name("stop").is_some());
    }

    #[test_log::test(tokio::test)]
    async fn test_agent_tool_calling_loop() {
        let prompt = "Write a poem";
        let mock_llm = MockChatCompletion::new();
        let mock_tool = MockTool::new();

        let chat_request = chat_request! {
            user!("Write a poem");

            tools = [mock_tool.clone()]
        };

        let mock_tool_response = chat_response! {
            "Roses are red";
            tool_calls = ["mock_tool"]

        };

        mock_llm.expect_complete(chat_request.clone(), Ok(mock_tool_response));

        let chat_request = chat_request! {
            user!("Write a poem"),
            assistant!("Roses are red"),
            tool_output!("mock_tool", "Great!");

            tools = [mock_tool.clone()]
        };

        let stop_response = chat_response! {
            "Roses are red";
            tool_calls = ["stop"]
        };

        mock_llm.expect_complete(chat_request, Ok(stop_response));
        mock_tool.expect_invoke("Great!".into(), None);

        let mut agent = Agent::builder()
            .tools([mock_tool])
            .llm(&mock_llm)
            .build()
            .unwrap();

        agent.query(prompt).await.unwrap();
    }

    #[test_log::test(tokio::test)]
    async fn test_agent_tool_run_once() {
        let prompt = "Write a poem";
        let mock_llm = MockChatCompletion::new();
        let mock_tool = MockTool::new();

        let chat_request = chat_request! {
            user!("Write a poem");

            tools = [mock_tool.clone()]
        };

        let mock_tool_response = chat_response! {
            "Roses are red";
            tool_calls = ["mock_tool"]

        };

        mock_tool.expect_invoke("Great!".into(), None);
        mock_llm.expect_complete(chat_request.clone(), Ok(mock_tool_response));

        let mut agent = Agent::builder()
            .tools([mock_tool])
            .llm(&mock_llm)
            .build()
            .unwrap();

        agent.query_once(prompt).await.unwrap();
    }

    #[test_log::test(tokio::test)]
    async fn test_agent_state_machine() {
        let prompt = "Write a poem";
        let mock_llm = MockChatCompletion::new();

        let chat_request = chat_request! {
            user!("Write a poem");
            tools = []
        };
        let mock_tool_response = chat_response! {
            "Roses are red";
            tool_calls = []
        };

        mock_llm.expect_complete(chat_request.clone(), Ok(mock_tool_response));
        let mut agent = Agent::builder().llm(&mock_llm).build().unwrap();

        // Agent has never run and is pending
        assert!(agent.state.is_pending());
        agent.query_once(prompt).await.unwrap();

        // Agent is stopped, there might be more messages
        assert!(agent.state.is_stopped());
    }

    #[test_log::test(tokio::test)]
    async fn test_agent_hooks() {
        let mock_before_all = MockHook::new().expect_calls(1).to_owned();
        let mock_before_each = MockHook::new().expect_calls(2).to_owned();
        let mock_after_each = MockHook::new().expect_calls(2).to_owned();

        // Once for mock tool and once for stop
        // let mock_after_tool = MockHook::new().expect_calls(2);

        let prompt = "Write a poem";
        let mock_llm = MockChatCompletion::new();
        let mock_tool = MockTool::new();

        let chat_request = chat_request! {
            user!("Write a poem");

            tools = [mock_tool.clone()]
        };

        let mock_tool_response = chat_response! {
            "Roses are red";
            tool_calls = ["mock_tool"]

        };

        mock_llm.expect_complete(chat_request.clone(), Ok(mock_tool_response));

        let chat_request = chat_request! {
            user!("Write a poem"),
            assistant!("Roses are red"),
            tool_output!("mock_tool", "Great!");

            tools = [mock_tool.clone()]
        };

        let stop_response = chat_response! {
            "Roses are red";
            tool_calls = ["stop"]
        };

        mock_llm.expect_complete(chat_request, Ok(stop_response));
        mock_tool.expect_invoke("Great!".into(), None);

        let mut agent = Agent::builder()
            .tools([mock_tool])
            .llm(&mock_llm)
            .before_all(mock_before_all.hook_fn())
            .before_each(mock_before_each.hook_fn())
            .after_each(mock_after_each.hook_fn())
            .build()
            .unwrap();

        agent.query(prompt).await.unwrap();
    }
}
