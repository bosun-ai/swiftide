#![allow(dead_code)]
use crate::{
    default_context::DefaultContext,
    hooks::{Hook, HookFn, HookTypes, MessageHookFn, ToolHookFn},
    state,
    system_prompt::SystemPrompt,
    tools::control::Stop,
};
use std::{collections::HashSet, sync::Arc};

use anyhow::Result;
use derive_builder::Builder;
use swiftide_core::{
    chat_completion::{
        errors::ToolError, ChatCompletion, ChatCompletionRequest, ChatMessage, Tool, ToolOutput,
    },
    prompt::Prompt,
    AgentContext,
};
use tokio::task::JoinHandle;
use tracing::debug;

// TODO:
// - [x] After calling run or run once cannot call run again
// - [x] Cannot call continue if agent has not called run (state machine?)
//       ... Or should we simplify it, and allow it for now?
// - [x] Agent should support a system prompt
// - [x] Hooks should  called at each correct point
// - [ ] Errors should all be thiserror and not anyhow
// - [ ] Improve tracing and logging (need to check when running it)
// - [ ] Consider making tools generic over context instead
//          NOTE: Makes async maybe easier? No cast from generic to dyn
// - [\] Ensure hooks can take both regular functions _and_ closures
//          NOTE: Partially works with explicit return of impl
// - [x] Add back history to context

// Notes
//
// Generic over LLM instead of box dyn? Should tool support be a separate trait?
#[derive(Clone, Builder)]
pub struct Agent {
    #[builder(default, setter(into))]
    pub(crate) hooks: Vec<Hook>,
    // name: String,
    #[builder(setter(custom), default = Arc::new(DefaultContext::default()))]
    pub(crate) context: Arc<dyn AgentContext>,
    #[builder(default = Agent::default_tools(), setter(custom))]
    pub(crate) tools: HashSet<Box<dyn Tool>>,

    #[builder(setter(custom))]
    pub(crate) llm: Box<dyn ChatCompletion>,

    /// System prompt for the agent when it starts
    ///
    /// Some agents profit significantly from a tailored prompt. But it is not always needed.
    ///
    /// See [`SystemPrompt`] for an opiniated, customizable system prompt.
    ///
    /// Swiftide provides a default system prompt for all agents.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use swiftide_agents::system_prompt::SystemPrompt;
    /// # use swiftide_agents::Agent;
    /// Agent::builder()
    ///     .system_prompt(
    ///         SystemPrompt::builder().role("You are an expert engineer")
    ///         .build().unwrap())
    ///     .build().unwrap();
    /// ```
    #[builder(setter(into, strip_option), default = Some(SystemPrompt::default().into()))]
    pub(crate) system_prompt: Option<Prompt>,

    #[builder(private, default = state::State::default())]
    pub(crate) state: state::State,
}

impl std::fmt::Debug for Agent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Agent")
            //display hooks as a list of type: number of hooks
            .field(
                "hooks",
                &self
                    .hooks
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>(),
            )
            .field(
                "tools",
                &self
                    .tools
                    .iter()
                    .map(|tool| tool.name())
                    .collect::<Vec<_>>(),
            )
            .field("llm", &"Box<dyn ChatCompletion>")
            .field("state", &self.state)
            .finish()
    }
}

impl AgentBuilder {
    pub fn context(&mut self, context: impl AgentContext + 'static) -> &mut AgentBuilder
    where
        Self: Clone,
    {
        self.context = Some(Arc::new(context));
        self
    }

    pub fn no_system_prompt(&mut self) -> &mut Self {
        self.system_prompt = Some(None);

        self
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

    pub fn after_tool(&mut self, hook: impl ToolHookFn + 'static) -> &mut Self {
        self.add_hook(Hook::AfterTool(Box::new(hook)))
    }

    pub fn after_each(&mut self, hook: impl HookFn + 'static) -> &mut Self {
        self.add_hook(Hook::AfterEach(Box::new(hook)))
    }

    pub fn on_new_message(&mut self, hook: impl MessageHookFn + 'static) -> &mut Self {
        self.add_hook(Hook::OnNewMessage(Box::new(hook)))
    }

    pub fn llm<LLM: ChatCompletion + Clone + 'static>(&mut self, llm: &LLM) -> &mut Self {
        let boxed: Box<dyn ChatCompletion> = Box::new(llm.clone());

        self.llm = Some(boxed);
        self
    }

    pub fn tools<TOOL, I: IntoIterator<Item = TOOL>>(&mut self, tools: I) -> &mut Self
    where
        TOOL: Into<Box<dyn Tool>>,
    {
        self.tools = Some(
            tools
                .into_iter()
                .map(Into::into)
                .chain(Agent::default_tools())
                .collect(),
        );
        self
    }
}

impl Agent {
    pub fn builder() -> AgentBuilder {
        AgentBuilder::default()
    }
}

impl Agent {
    fn default_tools() -> HashSet<Box<dyn Tool>> {
        HashSet::from([Box::new(Stop::default()) as Box<dyn Tool>])
    }

    #[tracing::instrument]
    pub async fn query(&mut self, query: impl Into<String> + std::fmt::Debug) -> Result<()> {
        self.run_agent(Some(query.into()), false).await
    }

    #[tracing::instrument]
    pub async fn query_once(&mut self, query: impl Into<String> + std::fmt::Debug) -> Result<()> {
        self.run_agent(Some(query.into()), true).await
    }

    #[tracing::instrument]
    pub async fn run(&mut self) -> Result<()> {
        self.run_agent(None, false).await
    }

    #[tracing::instrument]
    pub async fn run_once(&mut self) -> Result<()> {
        self.run_agent(None, true).await
    }

    pub async fn history(&self) -> Vec<ChatMessage> {
        self.context.history().await
    }

    // TODO: Inner mutability instead?
    async fn run_agent(&mut self, maybe_query: Option<String>, just_once: bool) -> Result<()> {
        if self.state.is_running() {
            anyhow::bail!("Agent is already running");
        }

        if self.state.is_pending() {
            if let Some(system_prompt) = &self.system_prompt {
                self.context
                    .add_messages(&[ChatMessage::System(system_prompt.render().await?)])
                    .await;
            }
            self.invoke_hooks_matching(HookTypes::BeforeAll).await?;
        }

        if let Some(query) = maybe_query {
            self.context.add_message(&ChatMessage::User(query)).await;
        }

        while let Some(messages) = self.context.next_completion().await {
            self.state = state::State::Running;

            let result = self.run_completions(&messages).await;

            if result.is_err() {
                self.state = state::State::Stopped;
                return result;
            }

            if just_once {
                break;
            }
        }

        self.state = state::State::Stopped;

        Ok(())
    }

    async fn run_completions(&self, messages: &[ChatMessage]) -> Result<()> {
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
                    .map(|tool| tool.tool_spec())
                    .collect::<HashSet<_>>(),
            )
            .build()?;

        debug!(
            "Calling LLM with request: {}",
            chat_completion_request
                .messages()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );
        let response = self.llm.complete(&chat_completion_request).await?;

        // let mut new_messages = vec![];
        if let Some(message) = response.message {
            debug!("LLM returned message: {}", message);

            self.add_message(ChatMessage::Assistant(message)).await?;
        }

        if let Some(tool_calls) = response.tool_calls {
            debug!("LLM returned tool calls: {:?}", tool_calls);

            let mut handles = vec![];
            for tool_call in tool_calls {
                let Some(tool) = self.find_tool_by_name(tool_call.name()) else {
                    tracing::warn!("Tool {} not found", tool_call.name());
                    continue;
                };
                tracing::info!("Calling tool `{}`", tool_call.name());

                let tool_args = tool_call.args().map(String::from);
                let context: Arc<dyn AgentContext> = Arc::clone(&self.context);

                self.add_message(ChatMessage::ToolCall(tool_call.clone()))
                    .await?;
                let handle: JoinHandle<Result<ToolOutput, ToolError>> = tokio::spawn(async move {
                    let output = tool.invoke(&*context, tool_args.as_deref()).await?;

                    Ok(output)
                });
                handles.push((handle, tool_call));
            }

            for (handle, tool_call) in handles {
                let mut output = handle.await?;

                for hook in self.hooks_by_type(HookTypes::AfterTool) {
                    if let Hook::AfterTool(hook) = hook {
                        tracing::info!("Calling {} hook", HookTypes::AfterTool);
                        hook(&*self.context, &tool_call, &mut output).await?;
                    }
                }

                let output = output?;

                self.handle_control_tools(&output);

                self.add_message(ChatMessage::ToolOutput(tool_call, output))
                    .await?;
            }
        };

        self.invoke_hooks_matching(HookTypes::AfterEach).await?;

        Ok(())
    }

    fn hooks_by_type(&self, hook_type: HookTypes) -> Vec<&Hook> {
        self.hooks
            .iter()
            .filter(|h| hook_type == (*h).into())
            .collect()
    }

    async fn invoke_hooks_matching(&self, hook_type: HookTypes) -> Result<()> {
        tracing::info!("Invoking {hook_type} hooks");

        for hook in self.hooks_by_type(hook_type) {
            match hook {
                Hook::BeforeAll(hook) => hook(&*self.context).await?,
                Hook::BeforeEach(hook) => hook(&*self.context).await?,
                Hook::AfterEach(hook) => hook(&*self.context).await?,
                Hook::AfterTool(..) | Hook::OnNewMessage(..) => {
                    debug_assert!(false, "Should not be called here");
                }
            }
        }

        Ok(())
    }

    fn find_tool_by_name(&self, tool_name: &str) -> Option<Box<dyn Tool>> {
        self.tools
            .iter()
            .find(|tool| tool.name() == tool_name)
            .cloned()
    }

    // Handle any tool specific output (e.g. stop)
    fn handle_control_tools(&self, output: &ToolOutput) {
        if let ToolOutput::Stop = output {
            self.context.stop();
        }
    }

    async fn add_message(&self, message: ChatMessage) -> Result<()> {
        for hook in self.hooks_by_type(HookTypes::OnNewMessage) {
            if let Hook::OnNewMessage(hook) = hook {
                hook(&*self.context, &message).await?;
            }
        }
        self.context.add_message(&message).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use swiftide_core::chat_completion::{ChatCompletionResponse, ToolCall};
    use swiftide_core::test_utils::MockChatCompletion;

    use super::*;
    use crate::{assistant, chat_request, chat_response, system, tool_call, tool_output, user};

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
            .tools([MockTool::new("mock_tool")])
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
        let mock_tool = MockTool::new("mock_tool");

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
            tool_call!("mock_tool"),
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
            .no_system_prompt()
            .build()
            .unwrap();

        agent.query(prompt).await.unwrap();
    }

    #[test_log::test(tokio::test)]
    async fn test_agent_tool_run_once() {
        let prompt = "Write a poem";
        let mock_llm = MockChatCompletion::new();
        let mock_tool = MockTool::default();

        let chat_request = chat_request! {
            system!("My system prompt"),
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
            .system_prompt("My system prompt")
            .llm(&mock_llm)
            .build()
            .unwrap();

        agent.query_once(prompt).await.unwrap();
    }

    #[test_log::test(tokio::test(flavor = "multi_thread"))]
    async fn test_multiple_tool_calls() {
        let prompt = "Write a poem";
        let mock_llm = MockChatCompletion::new();
        let mock_tool = MockTool::new("mock_tool1");
        let mock_tool2 = MockTool::new("mock_tool2");

        let chat_request = chat_request! {
            system!("My system prompt"),
            user!("Write a poem");



            tools = [mock_tool.clone(), mock_tool2.clone()]
        };

        let mock_tool_response = chat_response! {
            "Roses are red";

            tool_calls = ["mock_tool1", "mock_tool2"]

        };

        dbg!(&chat_request);
        mock_tool.expect_invoke("Great!".into(), None);
        mock_tool2.expect_invoke("Great!".into(), None);
        mock_llm.expect_complete(chat_request.clone(), Ok(mock_tool_response));

        let chat_request = chat_request! {
            system!("My system prompt"),
            user!("Write a poem"),
            assistant!("Roses are red"),
            tool_call!("mock_tool1"),
            tool_call!("mock_tool2"),
            tool_output!("mock_tool1", "Great!"),
            tool_output!("mock_tool2", "Great!");

            tools = [mock_tool.clone(), mock_tool2.clone()]
        };

        let mock_tool_response = chat_response! {
            "Ok!";

            tool_calls = ["stop"]

        };

        mock_llm.expect_complete(chat_request, Ok(mock_tool_response));

        let mut agent = Agent::builder()
            .tools([mock_tool, mock_tool2])
            .system_prompt("My system prompt")
            .llm(&mock_llm)
            .build()
            .unwrap();

        agent.query(prompt).await.unwrap();
    }

    async fn test_multiple_identical_tool_calls() {
        let prompt = "Write a poem";
        let mock_llm = MockChatCompletion::new();
        let mock_tool = MockTool::default();

        let chat_request = chat_request! {
            system!("My system prompt"),
            user!("Write a poem"),
            tool_call!("mock_tool"),
            tool_call!("mock_tool");

            tools = [mock_tool.clone()]
        };

        let mock_tool_response = chat_response! {
            "Roses are red";
            tool_calls = ["mock_tool", "mock_tool"]

        };

        mock_tool.expect_invoke("Great!".into(), None);
        mock_tool.expect_invoke("Great!".into(), None);
        mock_llm.expect_complete(chat_request.clone(), Ok(mock_tool_response));

        let chat_request = chat_request! {
            system!("My system prompt"),
            user!("Write a poem"),
            assistant!("Roses are red"),
            tool_call!("mock_tool"),
            tool_call!("mock_tool"),
            tool_output!("mock_tool", "Great!"),
            tool_output!("mock_tool", "Great!");

            tools = [mock_tool.clone()]
        };

        let mock_tool_response = chat_response! {
            "Ok!";

            tool_calls = ["stop"]

        };

        mock_llm.expect_complete(chat_request, Ok(mock_tool_response));
        let mut agent = Agent::builder()
            .tools([mock_tool])
            .system_prompt("My system prompt")
            .llm(&mock_llm)
            .build()
            .unwrap();
        agent.query(prompt).await.unwrap();
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
        let mut agent = Agent::builder()
            .llm(&mock_llm)
            .no_system_prompt()
            .build()
            .unwrap();

        // Agent has never run and is pending
        assert!(agent.state.is_pending());
        agent.query_once(prompt).await.unwrap();

        // Agent is stopped, there might be more messages
        assert!(agent.state.is_stopped());
    }

    #[test_log::test(tokio::test)]
    async fn test_agent_hooks() {
        let mock_before_all = MockHook::new("before_all").expect_calls(1).to_owned();
        let mock_before_each = MockHook::new("before_each").expect_calls(2).to_owned();
        let mock_after_each = MockHook::new("after_each").expect_calls(2).to_owned();
        let mock_on_message = MockHook::new("on_message").expect_calls(6).to_owned();

        // Once for mock tool and once for stop
        let mock_after_tool = MockHook::new("after_tool").expect_calls(2).to_owned();

        let prompt = "Write a poem";
        let mock_llm = MockChatCompletion::new();
        let mock_tool = MockTool::default();

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
            tool_call!("mock_tool"),
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
            .no_system_prompt()
            .before_all(mock_before_all.hook_fn())
            .before_each(mock_before_each.hook_fn())
            .after_each(mock_after_each.hook_fn())
            .after_tool(mock_after_tool.tool_hook_fn())
            .on_new_message(mock_on_message.message_hook_fn())
            .build()
            .unwrap();

        agent.query(prompt).await.unwrap();
    }
}
