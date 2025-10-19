#![allow(dead_code)]
use crate::{
    default_context::DefaultContext,
    errors::AgentError,
    hooks::{
        AfterCompletionFn, AfterEachFn, AfterToolFn, BeforeAllFn, BeforeCompletionFn, BeforeToolFn,
        Hook, HookTypes, MessageHookFn, OnStartFn, OnStopFn, OnStreamFn,
    },
    invoke_hooks,
    state::{self, StopReason},
    system_prompt::SystemPrompt,
    tools::{arg_preprocessor::ArgPreprocessor, control::Stop},
};
use std::{
    collections::{HashMap, HashSet},
    hash::{DefaultHasher, Hash as _, Hasher as _},
    sync::Arc,
};

use derive_builder::Builder;
use futures_util::stream::StreamExt;
use swiftide_core::{
    AgentContext, ToolBox,
    chat_completion::{
        ChatCompletion, ChatCompletionRequest, ChatMessage, Tool, ToolCall, ToolOutput,
    },
    prompt::Prompt,
};
use tracing::{Instrument, debug};

/// Agents are the main interface for building agentic systems.
///
/// Construct agents by calling the builder, setting an llm, configure hooks, tools and other
/// customizations.
///
/// # Important defaults
///
/// - The default context is the `DefaultContext`, executing tools locally with the `LocalExecutor`.
/// - A default `stop` tool is provided for agents to explicitly stop if needed
/// - The default `SystemPrompt` instructs the agent with chain of thought and some common
///   safeguards, but is otherwise quite bare. In a lot of cases this can be sufficient.
///
///   Agents are *not* cheap to clone. However, if an agent gets cloned, it will operate on the
///   same context.
#[derive(Builder)]
pub struct Agent {
    /// Hooks are functions that are called at specific points in the agent's lifecycle.
    #[builder(default, setter(into))]
    pub(crate) hooks: Vec<Hook>,
    /// The context in which the agent operates, by default this is the `DefaultContext`.
    #[builder(
        setter(custom),
        default = Arc::new(DefaultContext::default()) as Arc<dyn AgentContext>
    )]
    pub(crate) context: Arc<dyn AgentContext>,
    /// Tools the agent can use
    #[builder(default = Agent::default_tools(), setter(custom))]
    pub(crate) tools: HashSet<Box<dyn Tool>>,

    /// Toolboxes are collections of tools that can be added to the agent.
    ///
    /// Toolboxes make their tools available to the agent at runtime.
    #[builder(default)]
    pub(crate) toolboxes: Vec<Box<dyn ToolBox>>,

    /// The language model that the agent uses for completion.
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
    /// Alternatively you can also provide a `Prompt` directly, or disable the system prompt.
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
    #[builder(setter(into, strip_option), default = Some(SystemPrompt::default()))]
    pub(crate) system_prompt: Option<SystemPrompt>,

    /// Initial state of the agent
    #[builder(private, default = state::State::default())]
    pub(crate) state: state::State,

    /// Optional limit on the amount of loops the agent can run.
    /// The counter is reset when the agent is stopped.
    #[builder(default, setter(strip_option))]
    pub(crate) limit: Option<usize>,

    /// The maximum amount of times the failed output of a tool will be send
    /// to an LLM before the agent stops. Defaults to 3.
    ///
    /// LLMs sometimes send missing arguments, or a tool might actually fail, but retrying could be
    /// worth while. If the limit is not reached, the agent will send the formatted error back to
    /// the LLM.
    ///
    /// The limit is hashed based on the tool call name and arguments, so the limit is per tool
    /// call.
    ///
    /// This limit is _not_ reset when the agent is stopped.
    #[builder(default = 3)]
    pub(crate) tool_retry_limit: usize,

    /// Enables streaming the chat completion responses for the agent.
    #[builder(default)]
    pub(crate) streaming: bool,

    /// When set to true, any tools in `Agent::default_tools` will be omitted. Only works if you
    /// at at least one tool of your own.
    #[builder(private, default)]
    pub(crate) clear_default_tools: bool,

    /// Internally tracks the amount of times a tool has been retried. The key is a hash based on
    /// the name and args of the tool.
    #[builder(private, default)]
    pub(crate) tool_retries_counter: HashMap<u64, usize>,

    /// The name of the agent; optional
    #[builder(default = "unnamed_agent".into(), setter(into))]
    pub(crate) name: String,
}

impl Clone for Agent {
    fn clone(&self) -> Self {
        Agent {
            hooks: self.hooks.clone(),
            context: Arc::new(self.context.clone()),
            tools: self.tools.clone(),
            toolboxes: self.toolboxes.clone(),
            llm: self.llm.clone(),
            system_prompt: self.system_prompt.clone(),
            state: self.state.clone(),
            limit: self.limit,
            tool_retry_limit: self.tool_retry_limit,
            tool_retries_counter: HashMap::new(),
            streaming: self.streaming,
            name: self.name.clone(),
            clear_default_tools: self.clear_default_tools,
        }
    }
}

impl std::fmt::Debug for Agent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Agent")
            .field("name", &self.name)
            // display hooks as a list of type: number of hooks
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
                    .map(swiftide_core::Tool::name)
                    .collect::<Vec<_>>(),
            )
            .field("llm", &"Box<dyn ChatCompletion>")
            .field("state", &self.state)
            .finish()
    }
}

impl AgentBuilder {
    /// The context in which the agent operates, by default this is the `DefaultContext`.
    pub fn context(&mut self, context: impl AgentContext + 'static) -> &mut AgentBuilder
    where
        Self: Clone,
    {
        self.context = Some(Arc::new(context) as Arc<dyn AgentContext>);
        self
    }

    /// Returns a mutable reference to the system prompt, if it is set.
    pub fn system_prompt_mut(&mut self) -> Option<&mut SystemPrompt> {
        self.system_prompt.as_mut().and_then(Option::as_mut)
    }

    /// Disable the system prompt.
    pub fn no_system_prompt(&mut self) -> &mut Self {
        self.system_prompt = Some(None);

        self
    }

    /// Add a hook to the agent.
    pub fn add_hook(&mut self, hook: Hook) -> &mut Self {
        let hooks = self.hooks.get_or_insert_with(Vec::new);
        hooks.push(hook);

        self
    }

    /// Adds a tool to the agent
    pub fn add_tool(&mut self, tool: impl Tool + 'static) -> &mut Self {
        let tools = self.tools.get_or_insert_with(HashSet::new);
        if let Some(tool) = tools.replace(tool.boxed()) {
            tracing::debug!("Tool {} already exists, replacing", tool.name());
        }

        self
    }

    /// Add a hook that runs once, before all completions. Even if the agent is paused and resumed,
    /// before all will not trigger again.
    pub fn before_all(&mut self, hook: impl BeforeAllFn + 'static) -> &mut Self {
        self.add_hook(Hook::BeforeAll(Box::new(hook)))
    }

    /// Add a hook that runs once, when the agent starts. This hook also runs if the agent stopped
    /// and then starts again. The hook runs after any `before_all` hooks and before the
    /// `before_completion` hooks.
    pub fn on_start(&mut self, hook: impl OnStartFn + 'static) -> &mut Self {
        self.add_hook(Hook::OnStart(Box::new(hook)))
    }

    /// Add a hook that runs when the agent receives a streaming response
    ///
    /// The response will always include both the current accumulated message and the delta
    ///
    /// This will set `self.streaming` to true, there is no need to set it manually for the default
    /// behaviour.
    pub fn on_stream(&mut self, hook: impl OnStreamFn + 'static) -> &mut Self {
        self.streaming = Some(true);
        self.add_hook(Hook::OnStream(Box::new(hook)))
    }

    /// Add a hook that runs before each completion.
    pub fn before_completion(&mut self, hook: impl BeforeCompletionFn + 'static) -> &mut Self {
        self.add_hook(Hook::BeforeCompletion(Box::new(hook)))
    }

    /// Add a hook that runs after each tool. The `Result<ToolOutput, ToolError>` is provided
    /// as mut, so the tool output can be fully modified.
    ///
    /// The `ToolOutput` also references the original `ToolCall`, allowing you to match at runtime
    /// what tool to interact with.
    pub fn after_tool(&mut self, hook: impl AfterToolFn + 'static) -> &mut Self {
        self.add_hook(Hook::AfterTool(Box::new(hook)))
    }

    /// Add a hook that runs before each tool. Yields an immutable reference to the `ToolCall`.
    pub fn before_tool(&mut self, hook: impl BeforeToolFn + 'static) -> &mut Self {
        self.add_hook(Hook::BeforeTool(Box::new(hook)))
    }

    /// Add a hook that runs after each completion, before tool invocation and/or new messages.
    pub fn after_completion(&mut self, hook: impl AfterCompletionFn + 'static) -> &mut Self {
        self.add_hook(Hook::AfterCompletion(Box::new(hook)))
    }

    /// Add a hook that runs after each completion, after tool invocations, right before a new loop
    /// might start
    pub fn after_each(&mut self, hook: impl AfterEachFn + 'static) -> &mut Self {
        self.add_hook(Hook::AfterEach(Box::new(hook)))
    }

    /// Add a hook that runs when a new message is added to the context. Note that each tool adds a
    /// separate message.
    pub fn on_new_message(&mut self, hook: impl MessageHookFn + 'static) -> &mut Self {
        self.add_hook(Hook::OnNewMessage(Box::new(hook)))
    }

    pub fn on_stop(&mut self, hook: impl OnStopFn + 'static) -> &mut Self {
        self.add_hook(Hook::OnStop(Box::new(hook)))
    }

    /// Set the LLM for the agent. An LLM must implement the `ChatCompletion` trait.
    pub fn llm<LLM: ChatCompletion + Clone + 'static>(&mut self, llm: &LLM) -> &mut Self {
        let boxed: Box<dyn ChatCompletion> = Box::new(llm.clone()) as Box<dyn ChatCompletion>;

        self.llm = Some(boxed);
        self
    }

    /// Removes the default `stop` tool from the agent. This allows you to add your own or use
    /// other methods to stop the agent.
    ///
    /// Note that you can also just override the tool if the name of the tool is `stop`.
    pub fn without_default_stop_tool(&mut self) -> &mut Self {
        self.clear_default_tools = Some(true);
        self
    }

    fn builder_default_tools(&self) -> HashSet<Box<dyn Tool>> {
        if self.clear_default_tools.is_some_and(|b| b) {
            HashSet::new()
        } else {
            Agent::default_tools()
        }
    }

    /// Define the available tools for the agent. Tools must implement the `Tool` trait.
    ///
    /// See the [tool attribute macro](`swiftide_macros::tool`) and the [tool derive
    /// macro](`swiftide_macros::Tool`) for easy ways to create (many) tools.
    pub fn tools<TOOL, I: IntoIterator<Item = TOOL>>(&mut self, tools: I) -> &mut Self
    where
        TOOL: Into<Box<dyn Tool>>,
    {
        self.tools = Some(
            self.builder_default_tools()
                .into_iter()
                .chain(tools.into_iter().map(Into::into))
                .collect(),
        );
        self
    }

    /// Add a toolbox to the agent. Toolboxes are collections of tools that can be added to the
    /// to the agent. Available tools are evaluated at runtime, when the agent starts for the first
    /// time.
    ///
    /// Agents can have many toolboxes.
    pub fn add_toolbox(&mut self, toolbox: impl ToolBox + 'static) -> &mut Self {
        let toolboxes = self.toolboxes.get_or_insert_with(Vec::new);
        toolboxes.push(Box::new(toolbox));

        self
    }
}

impl Agent {
    /// Build a new agent
    pub fn builder() -> AgentBuilder {
        AgentBuilder::default()
            .tools(Agent::default_tools())
            .to_owned()
    }

    /// The name of the agent
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Default tools for the agent that it always includes
    /// Right now this is the `stop` tool, which allows the agent to stop itself.
    pub fn default_tools() -> HashSet<Box<dyn Tool>> {
        HashSet::from([Stop::default().boxed()])
    }

    /// Run the agent with a user message. The agent will loop completions, make tool calls, until
    /// no new messages are available.
    ///
    /// # Errors
    ///
    /// Errors if anything goes wrong, see `AgentError` for more details.
    #[tracing::instrument(skip_all, name = "agent.query", err)]
    pub async fn query(&mut self, query: impl Into<Prompt>) -> Result<(), AgentError> {
        let query = query
            .into()
            .render()
            .map_err(AgentError::FailedToRenderPrompt)?;
        self.run_agent(Some(query), false).await
    }

    /// Adds a tool to an agent at run time
    pub fn add_tool(&mut self, tool: Box<dyn Tool>) {
        if let Some(tool) = self.tools.replace(tool) {
            tracing::debug!("Tool {} already exists, replacing", tool.name());
        }
    }

    /// Modify the tools of the agent at runtime
    ///
    /// Note that any mcp tools are added to the agent after the first start, and will only then
    /// also be available here.
    pub fn tools_mut(&mut self) -> &mut HashSet<Box<dyn Tool>> {
        &mut self.tools
    }

    /// Run the agent with a user message once.
    ///
    /// # Errors
    ///
    /// Errors if anything goes wrong, see `AgentError` for more details.
    #[tracing::instrument(skip_all, name = "agent.query_once", err)]
    pub async fn query_once(&mut self, query: impl Into<Prompt>) -> Result<(), AgentError> {
        self.run_agent(Some(query), true).await
    }

    /// Run the agent with without user message. The agent will loop completions, make tool calls,
    /// until no new messages are available.
    ///
    /// # Errors
    ///
    /// Errors if anything goes wrong, see `AgentError` for more details.
    #[tracing::instrument(skip_all, name = "agent.run", err)]
    pub async fn run(&mut self) -> Result<(), AgentError> {
        self.run_agent(None::<Prompt>, false).await
    }

    /// Run the agent with without user message. The agent will loop completions, make tool calls,
    /// until
    ///
    /// # Errors
    ///
    /// Errors if anything goes wrong, see `AgentError` for more details.
    #[tracing::instrument(skip_all, name = "agent.run_once", err)]
    pub async fn run_once(&mut self) -> Result<(), AgentError> {
        self.run_agent(None::<Prompt>, true).await
    }

    /// Retrieve the message history of the agent
    ///
    /// # Errors
    ///
    /// Error if the message history cannot be retrieved, e.g. if the context is not set up or a
    /// connection fails
    pub async fn history(&self) -> Result<Vec<ChatMessage>, AgentError> {
        self.context
            .history()
            .await
            .map_err(AgentError::MessageHistoryError)
    }

    pub(crate) async fn run_agent(
        &mut self,
        maybe_query: Option<impl Into<Prompt>>,
        just_once: bool,
    ) -> Result<(), AgentError> {
        let maybe_query = maybe_query
            .map(|q| q.into().render())
            .transpose()
            .map_err(AgentError::FailedToRenderPrompt)?;
        if self.state.is_running() {
            return Err(AgentError::AlreadyRunning);
        }

        if self.state.is_pending() {
            if let Some(system_prompt) = &self.system_prompt {
                self.context
                    .add_messages(vec![ChatMessage::System(
                        system_prompt
                            .to_prompt()
                            .render()
                            .map_err(AgentError::FailedToRenderSystemPrompt)?,
                    )])
                    .await
                    .map_err(AgentError::MessageHistoryError)?;
            }

            invoke_hooks!(BeforeAll, self);

            self.load_toolboxes().await?;
        }

        if let Some(query) = maybe_query {
            if cfg!(feature = "langfuse") {
                debug!(langfuse.input = query);
            }
            self.context
                .add_message(ChatMessage::User(query))
                .await
                .map_err(AgentError::MessageHistoryError)?;
        }

        invoke_hooks!(OnStart, self);

        self.state = state::State::Running;

        let mut loop_counter = 0;

        while let Some(messages) = self
            .context
            .next_completion()
            .await
            .map_err(AgentError::MessageHistoryError)?
        {
            if let Some(limit) = self.limit
                && loop_counter >= limit
            {
                tracing::warn!("Agent loop limit reached");
                break;
            }

            // If the last message contains tool calls that have not been completed,
            // run the tools first
            if let Some(&ChatMessage::Assistant(.., Some(ref tool_calls))) =
                maybe_tool_call_without_output(&messages)
            {
                tracing::debug!("Uncompleted tool calls found; invoking tools");
                self.invoke_tools(tool_calls).await?;
                // Move on to the next tick, so that the
                continue;
            }

            let result = self.step(&messages, loop_counter).await;

            if let Err(err) = result {
                self.stop_with_error(&err).await;
                tracing::error!(error = ?err, "Agent stopped with error {err}");
                return Err(err);
            }

            if just_once || self.state.is_stopped() {
                break;
            }
            loop_counter += 1;
        }

        // If there are no new messages, ensure we update our state
        self.stop(StopReason::NoNewMessages).await;

        Ok(())
    }

    #[tracing::instrument(skip(self, messages), err, fields(otel.name))]
    async fn step(
        &mut self,
        messages: &[ChatMessage],
        step_count: usize,
    ) -> Result<(), AgentError> {
        tracing::Span::current().record("otel.name", format!("step-{step_count}"));

        debug!(
            tools = ?self
                .tools
                .iter()
                .map(|t| t.name())
                .collect::<Vec<_>>()
                ,
            "Running completion for agent with {} new messages",
            messages.len()
        );

        let mut chat_completion_request = ChatCompletionRequest::builder()
            .messages(messages.to_vec())
            .tool_specs(self.tools.iter().map(swiftide_core::Tool::tool_spec))
            .build()
            .map_err(AgentError::FailedToBuildRequest)?;

        invoke_hooks!(BeforeCompletion, self, &mut chat_completion_request);

        debug!(
            "Calling LLM with the following new messages:\n {}",
            self.context
                .current_new_messages()
                .await
                .map_err(AgentError::MessageHistoryError)?
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",\n")
        );

        let mut response = if self.streaming {
            let mut last_response = None;
            let mut stream = self.llm.complete_stream(&chat_completion_request).await;

            while let Some(response) = stream.next().await {
                let response = response.map_err(AgentError::CompletionsFailed)?;
                invoke_hooks!(OnStream, self, &response);
                last_response = Some(response);
            }
            tracing::trace!(?last_response, "Streaming completed");
            last_response.ok_or(AgentError::EmptyStream)
        } else {
            self.llm
                .complete(&chat_completion_request)
                .await
                .map_err(AgentError::CompletionsFailed)
        }?;

        // The arg preprocessor helps avoid common llm errors.
        // This must happen as early as possible
        response
            .tool_calls
            .as_deref_mut()
            .map(ArgPreprocessor::preprocess_tool_calls);

        invoke_hooks!(AfterCompletion, self, &mut response);

        self.add_message(ChatMessage::Assistant(
            response.message,
            response.tool_calls.clone(),
        ))
        .await?;

        if let Some(tool_calls) = response.tool_calls {
            self.invoke_tools(&tool_calls).await?;
        }

        invoke_hooks!(AfterEach, self);

        Ok(())
    }

    async fn invoke_tools(&mut self, tool_calls: &[ToolCall]) -> Result<(), AgentError> {
        tracing::debug!("LLM returned tool calls: {:?}", tool_calls);

        let mut handles = vec![];
        for tool_call in tool_calls {
            let Some(tool) = self.find_tool_by_name(tool_call.name()) else {
                tracing::warn!("Tool {} not found", tool_call.name());
                continue;
            };
            tracing::info!("Calling tool `{}`", tool_call.name());

            // let tool_args = tool_call.args().map(String::from);
            let context: Arc<dyn AgentContext> = Arc::clone(&self.context);

            invoke_hooks!(BeforeTool, self, &tool_call);

            let tool_span = tracing::info_span!(
                "tool",
                "otel.name" = format!("tool.{}", tool.name().as_ref()),
            );

            let handle_tool_call = tool_call.clone();
            let handle = tokio::spawn(async move {
                    let handle_tool_call = handle_tool_call;
                    let output = tool.invoke(&*context, &handle_tool_call)
                        .await?;

                if cfg!(feature = "langfuse") {
                    tracing::debug!(
                        langfuse.output = %output,
                        langfuse.input = handle_tool_call.args(),
                        tool_name = tool.name().as_ref(),
                    );
                } else {
                    tracing::debug!(output = output.to_string(), args = ?handle_tool_call.args(), tool_name = tool.name().as_ref(), "Completed tool call");
                }

                    Ok(output)
                }.instrument(tool_span.or_current()));

            handles.push((handle, tool_call));
        }

        for (handle, tool_call) in handles {
            let mut output = handle
                .await
                .map_err(|err| AgentError::ToolFailedToJoin(tool_call.name().to_string(), err))?;

            invoke_hooks!(AfterTool, self, &tool_call, &mut output);

            if let Err(error) = output {
                let stop = self.tool_calls_over_limit(tool_call);
                if stop {
                    tracing::error!(
                        ?error,
                        "Tool call failed, retry limit reached, stopping agent: {error}",
                    );
                } else {
                    tracing::warn!(
                        ?error,
                        tool_call = ?tool_call,
                        "Tool call failed, retrying",
                    );
                }
                self.add_message(ChatMessage::ToolOutput(
                    tool_call.clone(),
                    ToolOutput::fail(error.to_string()),
                ))
                .await?;
                if stop {
                    self.stop(StopReason::ToolCallsOverLimit(tool_call.to_owned()))
                        .await;
                    return Err(error.into());
                }
                continue;
            }

            let output = output?;
            self.handle_control_tools(tool_call, &output).await;

            // Feedback required leaves the tool call open
            //
            // It assumes a follow up invocation of the agent will have the feedback approved
            if !output.is_feedback_required() {
                self.add_message(ChatMessage::ToolOutput(tool_call.to_owned(), output))
                    .await?;
            }
        }

        Ok(())
    }

    fn hooks_by_type(&self, hook_type: HookTypes) -> Vec<&Hook> {
        self.hooks
            .iter()
            .filter(|h| hook_type == (*h).into())
            .collect()
    }

    fn find_tool_by_name(&self, tool_name: &str) -> Option<Box<dyn Tool>> {
        self.tools
            .iter()
            .find(|tool| tool.name() == tool_name)
            .cloned()
    }

    // Handle any tool specific output (e.g. stop)
    async fn handle_control_tools(&mut self, tool_call: &ToolCall, output: &ToolOutput) {
        match output {
            ToolOutput::Stop(maybe_message) => {
                tracing::warn!("Stop tool called, stopping agent");
                self.stop(StopReason::RequestedByTool(
                    tool_call.clone(),
                    maybe_message.clone(),
                ))
                .await;
            }
            ToolOutput::FeedbackRequired(maybe_payload) => {
                tracing::warn!("Feedback required, stopping agent");
                self.stop(StopReason::FeedbackRequired {
                    tool_call: tool_call.clone(),
                    payload: maybe_payload.clone(),
                })
                .await;
            }
            ToolOutput::AgentFailed(output) => {
                tracing::warn!("Agent failed, stopping agent");
                self.stop(StopReason::AgentFailed(output.clone())).await;
            }
            _ => (),
        }
    }

    /// Retrieve the system prompt, if it is set.
    pub fn system_prompt(&self) -> Option<&SystemPrompt> {
        self.system_prompt.as_ref()
    }

    /// Retrieve a mutable reference to the system prompt, if it is set.
    ///
    /// Note that the system prompt is rendered only once, when the agent starts for the first time
    pub fn system_prompt_mut(&mut self) -> Option<&mut SystemPrompt> {
        self.system_prompt.as_mut()
    }

    fn tool_calls_over_limit(&mut self, tool_call: &ToolCall) -> bool {
        let mut s = DefaultHasher::new();
        tool_call.hash(&mut s);
        let hash = s.finish();

        if let Some(retries) = self.tool_retries_counter.get_mut(&hash) {
            let val = *retries >= self.tool_retry_limit;
            *retries += 1;
            val
        } else {
            self.tool_retries_counter.insert(hash, 1);
            false
        }
    }

    /// Add a message to the agent's context
    ///
    /// This will trigger a `OnNewMessage` hook if its present.
    ///
    /// If you want to add a message without triggering the hook, use the context directly.
    ///
    /// # Errors
    ///
    /// Errors if the message cannot be added to the context. With the default in memory context
    /// that is not supposed to happen.
    #[tracing::instrument(skip_all, fields(message = message.to_string()))]
    pub async fn add_message(&self, mut message: ChatMessage) -> Result<(), AgentError> {
        invoke_hooks!(OnNewMessage, self, &mut message);

        self.context
            .add_message(message)
            .await
            .map_err(AgentError::MessageHistoryError)?;
        Ok(())
    }

    /// Tell the agent to stop. It will finish it's current loop and then stop.
    pub async fn stop(&mut self, reason: impl Into<StopReason>) {
        if self.state.is_stopped() {
            return;
        }

        let reason = reason.into();
        invoke_hooks!(OnStop, self, reason.clone(), None);

        if cfg!(feature = "langfuse") {
            debug!(langfuse.output = serde_json::to_string_pretty(&reason).ok());
        }

        self.state = state::State::Stopped(reason);
    }

    pub async fn stop_with_error(&mut self, error: &AgentError) {
        if self.state.is_stopped() {
            return;
        }
        invoke_hooks!(OnStop, self, StopReason::Error, Some(error));

        self.state = state::State::Stopped(StopReason::Error);
    }

    /// Access the agent's context
    pub fn context(&self) -> &dyn AgentContext {
        &self.context
    }

    /// The agent is still running
    pub fn is_running(&self) -> bool {
        self.state.is_running()
    }

    /// The agent stopped
    pub fn is_stopped(&self) -> bool {
        self.state.is_stopped()
    }

    /// The agent has not (ever) started
    pub fn is_pending(&self) -> bool {
        self.state.is_pending()
    }

    /// Get a list of tools available to the agent
    pub fn tools(&self) -> &HashSet<Box<dyn Tool>> {
        &self.tools
    }

    pub fn state(&self) -> &state::State {
        &self.state
    }

    pub fn stop_reason(&self) -> Option<&StopReason> {
        self.state.stop_reason()
    }

    async fn load_toolboxes(&mut self) -> Result<(), AgentError> {
        for toolbox in &self.toolboxes {
            let tools = toolbox
                .available_tools()
                .await
                .map_err(AgentError::ToolBoxFailedToLoad)?;
            self.tools.extend(tools);
        }

        Ok(())
    }
}

/// Reverse searches through messages, if it encounters a tool call before encountering an output,
/// it will return the chat message with the tool calls, otherwise it returns None
fn maybe_tool_call_without_output(messages: &[ChatMessage]) -> Option<&ChatMessage> {
    for message in messages.iter().rev() {
        if let ChatMessage::ToolOutput(..) = message {
            return None;
        }

        if let ChatMessage::Assistant(.., Some(tool_calls)) = message
            && !tool_calls.is_empty()
        {
            return Some(message);
        }
    }

    None
}

#[cfg(test)]
mod tests {

    use serde::ser::Error;
    use swiftide_core::ToolFeedback;
    use swiftide_core::chat_completion::errors::ToolError;
    use swiftide_core::chat_completion::{ChatCompletionResponse, ToolCall};
    use swiftide_core::test_utils::MockChatCompletion;

    use super::*;
    use crate::{
        State, assistant, chat_request, chat_response, summary, system, tool_failed, tool_output,
        user,
    };

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

        assert!(agent.context().history().await.unwrap().is_empty());
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
            assistant!("Roses are red", ["mock_tool"]),
            tool_output!("mock_tool", "Great!");

            tools = [mock_tool.clone()]
        };

        let stop_response = chat_response! {
            "Roses are red";
            tool_calls = ["stop"]
        };

        mock_llm.expect_complete(chat_request, Ok(stop_response));
        mock_tool.expect_invoke_ok("Great!".into(), None);

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

        mock_tool.expect_invoke_ok("Great!".into(), None);
        mock_llm.expect_complete(chat_request.clone(), Ok(mock_tool_response));

        let mut agent = Agent::builder()
            .tools([mock_tool])
            .system_prompt("My system prompt")
            .llm(&mock_llm)
            .build()
            .unwrap();

        agent.query_once(prompt).await.unwrap();
    }

    #[test_log::test(tokio::test)]
    async fn test_agent_tool_via_toolbox_run_once() {
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

        mock_tool.expect_invoke_ok("Great!".into(), None);
        mock_llm.expect_complete(chat_request.clone(), Ok(mock_tool_response));

        let mut agent = Agent::builder()
            .add_toolbox(vec![mock_tool.boxed()])
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
        mock_tool.expect_invoke_ok("Great!".into(), None);
        mock_tool2.expect_invoke_ok("Great!".into(), None);
        mock_llm.expect_complete(chat_request.clone(), Ok(mock_tool_response));

        let chat_request = chat_request! {
            system!("My system prompt"),
            user!("Write a poem"),
            assistant!("Roses are red", ["mock_tool1", "mock_tool2"]),
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
    async fn test_summary() {
        let prompt = "Write a poem";
        let mock_llm = MockChatCompletion::new();

        let mock_tool_response = chat_response! {
            "Roses are red";
            tool_calls = []

        };

        let expected_chat_request = chat_request! {
            system!("My system prompt"),
            user!("Write a poem");

            tools = []
        };

        mock_llm.expect_complete(expected_chat_request, Ok(mock_tool_response.clone()));

        let mut agent = Agent::builder()
            .system_prompt("My system prompt")
            .llm(&mock_llm)
            .build()
            .unwrap();

        agent.query_once(prompt).await.unwrap();

        agent
            .context
            .add_message(ChatMessage::new_summary("Summary"))
            .await
            .unwrap();

        let expected_chat_request = chat_request! {
            system!("My system prompt"),
            summary!("Summary"),
            user!("Write another poem");
            tools = []
        };
        mock_llm.expect_complete(expected_chat_request, Ok(mock_tool_response.clone()));

        agent.query_once("Write another poem").await.unwrap();

        agent
            .context
            .add_message(ChatMessage::new_summary("Summary 2"))
            .await
            .unwrap();

        let expected_chat_request = chat_request! {
            system!("My system prompt"),
            summary!("Summary 2"),
            user!("Write a third poem");
            tools = []
        };
        mock_llm.expect_complete(expected_chat_request, Ok(mock_tool_response));

        agent.query_once("Write a third poem").await.unwrap();
    }

    #[test_log::test(tokio::test)]
    async fn test_agent_hooks() {
        let mock_before_all = MockHook::new("before_all").expect_calls(1).to_owned();
        let mock_on_start_fn = MockHook::new("on_start").expect_calls(1).to_owned();
        let mock_before_completion = MockHook::new("before_completion")
            .expect_calls(2)
            .to_owned();
        let mock_after_completion = MockHook::new("after_completion").expect_calls(2).to_owned();
        let mock_after_each = MockHook::new("after_each").expect_calls(2).to_owned();
        let mock_on_message = MockHook::new("on_message").expect_calls(4).to_owned();
        let mock_on_stop = MockHook::new("on_stop").expect_calls(1).to_owned();

        // Once for mock tool and once for stop
        let mock_before_tool = MockHook::new("before_tool").expect_calls(2).to_owned();
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
            assistant!("Roses are red", ["mock_tool"]),
            tool_output!("mock_tool", "Great!");

            tools = [mock_tool.clone()]
        };

        let stop_response = chat_response! {
            "Roses are red";
            tool_calls = ["stop"]
        };

        mock_llm.expect_complete(chat_request, Ok(stop_response));
        mock_tool.expect_invoke_ok("Great!".into(), None);

        let mut agent = Agent::builder()
            .tools([mock_tool])
            .llm(&mock_llm)
            .no_system_prompt()
            .before_all(mock_before_all.hook_fn())
            .on_start(mock_on_start_fn.on_start_fn())
            .before_completion(mock_before_completion.before_completion_fn())
            .before_tool(mock_before_tool.before_tool_fn())
            .after_completion(mock_after_completion.after_completion_fn())
            .after_tool(mock_after_tool.after_tool_fn())
            .after_each(mock_after_each.hook_fn())
            .on_new_message(mock_on_message.message_hook_fn())
            .on_stop(mock_on_stop.stop_hook_fn())
            .build()
            .unwrap();

        agent.query(prompt).await.unwrap();
    }

    #[test_log::test(tokio::test)]
    async fn test_agent_loop_limit() {
        let prompt = "Generate content"; // Example prompt
        let mock_llm = MockChatCompletion::new();
        let mock_tool = MockTool::new("mock_tool");

        let chat_request = chat_request! {
            user!(prompt);
            tools = [mock_tool.clone()]
        };
        mock_tool.expect_invoke_ok("Great!".into(), None);

        let mock_tool_response = chat_response! {
            "Some response";
            tool_calls = ["mock_tool"]
        };

        // Set expectations for the mock LLM responses
        mock_llm.expect_complete(chat_request.clone(), Ok(mock_tool_response.clone()));

        // // Response for terminating the loop
        let stop_response = chat_response! {
            "Final response";
            tool_calls = ["stop"]
        };

        mock_llm.expect_complete(chat_request, Ok(stop_response));

        let mut agent = Agent::builder()
            .tools([mock_tool])
            .llm(&mock_llm)
            .no_system_prompt()
            .limit(1) // Setting the loop limit to 1
            .build()
            .unwrap();

        // Run the agent
        agent.query(prompt).await.unwrap();

        // Assert that the remaining message is still in the queue
        let remaining = mock_llm.expectations.lock().unwrap().pop();
        assert!(remaining.is_some());

        // Assert that the agent is stopped after reaching the loop limit
        assert!(agent.is_stopped());
    }

    #[test_log::test(tokio::test)]
    async fn test_tool_retry_mechanism() {
        let prompt = "Execute my tool";
        let mock_llm = MockChatCompletion::new();
        let mock_tool = MockTool::new("retry_tool");

        // Configure mock tool to fail twice. First time is fed back to the LLM, second time is an
        // error
        mock_tool.expect_invoke(
            Err(ToolError::WrongArguments(serde_json::Error::custom(
                "missing `query`",
            ))),
            None,
        );
        mock_tool.expect_invoke(
            Err(ToolError::WrongArguments(serde_json::Error::custom(
                "missing `query`",
            ))),
            None,
        );

        let chat_request = chat_request! {
            user!(prompt);
            tools = [mock_tool.clone()]
        };
        let retry_response = chat_response! {
            "First failing attempt";
            tool_calls = ["retry_tool"]
        };
        mock_llm.expect_complete(chat_request.clone(), Ok(retry_response));

        let chat_request = chat_request! {
            user!(prompt),
            assistant!("First failing attempt", ["retry_tool"]),
            tool_failed!("retry_tool", "arguments for tool failed to parse: missing `query`");

            tools = [mock_tool.clone()]
        };
        let will_fail_response = chat_response! {
            "Finished execution";
            tool_calls = ["retry_tool"]
        };
        mock_llm.expect_complete(chat_request.clone(), Ok(will_fail_response));

        let mut agent = Agent::builder()
            .tools([mock_tool])
            .llm(&mock_llm)
            .no_system_prompt()
            .tool_retry_limit(1) // The test relies on a limit of 2 retries.
            .build()
            .unwrap();

        // Run the agent
        let result = agent.query(prompt).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing `query`"));
        assert!(agent.is_stopped());
    }

    #[test_log::test(tokio::test(flavor = "multi_thread"))]
    async fn test_streaming() {
        let prompt = "Generate content"; // Example prompt
        let mock_llm = MockChatCompletion::new();
        let on_stream_fn = MockHook::new("on_stream").expect_calls(3).to_owned();

        let chat_request = chat_request! {
            user!(prompt);

            tools = []
        };

        let response = chat_response! {
            "one two three";
            tool_calls = ["stop"]
        };

        // Set expectations for the mock LLM responses
        mock_llm.expect_complete(chat_request, Ok(response));

        let mut agent = Agent::builder()
            .llm(&mock_llm)
            .on_stream(on_stream_fn.on_stream_fn())
            .no_system_prompt()
            .build()
            .unwrap();

        // Run the agent
        agent.query(prompt).await.unwrap();

        tracing::debug!("Agent finished running");

        // Assert that the agent is stopped after reaching the loop limit
        assert!(agent.is_stopped());
    }

    #[test_log::test(tokio::test)]
    async fn test_recovering_agent_existing_history() {
        // First, let's run an agent
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
            assistant!("Roses are red", ["mock_tool"]),
            tool_output!("mock_tool", "Great!");

            tools = [mock_tool.clone()]
        };

        let stop_response = chat_response! {
            "Roses are red";
            tool_calls = ["stop"]
        };

        mock_llm.expect_complete(chat_request, Ok(stop_response));
        mock_tool.expect_invoke_ok("Great!".into(), None);

        let mut agent = Agent::builder()
            .tools([mock_tool.clone()])
            .llm(&mock_llm)
            .no_system_prompt()
            .build()
            .unwrap();

        agent.query(prompt).await.unwrap();

        // Let's retrieve the history of the agent
        let history = agent.history().await.unwrap();

        // Store it as a string somewhere
        let serialized = serde_json::to_string(&history).unwrap();

        // Retrieve it
        let history: Vec<ChatMessage> = serde_json::from_str(&serialized).unwrap();

        // Build a context from the history
        let context = DefaultContext::default()
            .with_existing_messages(history)
            .await
            .unwrap()
            .to_owned();

        let stop_output = ToolOutput::stop();
        let expected_chat_request = chat_request! {
            user!("Write a poem"),
            assistant!("Roses are red", ["mock_tool"]),
            tool_output!("mock_tool", "Great!"),
            assistant!("Roses are red", ["stop"]),
            tool_output!("stop", stop_output),
            user!("Try again!");

            tools = [mock_tool.clone()]
        };

        let stop_response = chat_response! {
            "Really stopping now";
            tool_calls = ["stop"]
        };

        mock_llm.expect_complete(expected_chat_request, Ok(stop_response));

        let mut agent = Agent::builder()
            .context(context)
            .tools([mock_tool])
            .llm(&mock_llm)
            .no_system_prompt()
            .build()
            .unwrap();

        agent.query_once("Try again!").await.unwrap();
    }

    #[test_log::test(tokio::test)]
    async fn test_agent_with_approval_required_tool() {
        use super::*;
        use crate::tools::control::ApprovalRequired;
        use crate::{assistant, chat_request, chat_response, user};
        use swiftide_core::chat_completion::ToolCall;

        // Step 1: Build a tool that needs approval.
        let mock_tool = MockTool::default();
        mock_tool.expect_invoke_ok("Great!".into(), None);

        let approval_tool = ApprovalRequired(mock_tool.boxed());

        // Step 2: Set up the mock LLM.
        let mock_llm = MockChatCompletion::new();

        let chat_req1 = chat_request! {
            user!("Request with approval");
            tools = [approval_tool.clone()]
        };
        let chat_resp1 = chat_response! {
            "Completion message";
            tool_calls = ["mock_tool"]
        };
        mock_llm.expect_complete(chat_req1.clone(), Ok(chat_resp1));

        // The response will include the previous request, but no tool output
        // from the required tool
        let chat_req2 = chat_request! {
            user!("Request with approval"),
            assistant!("Completion message", ["mock_tool"]),
            tool_output!("mock_tool", "Great!");
            // Simulate feedback required output
            tools = [approval_tool.clone()]
        };
        let chat_resp2 = chat_response! {
            "Post-feedback message";
            tool_calls = ["stop"]
        };
        mock_llm.expect_complete(chat_req2.clone(), Ok(chat_resp2));

        // Step 3: Wire up the agent.
        let mut agent = Agent::builder()
            .tools([approval_tool])
            .llm(&mock_llm)
            .no_system_prompt()
            .build()
            .unwrap();

        // Step 4: Run agent to trigger approval.
        agent.query_once("Request with approval").await.unwrap();

        assert!(matches!(
            agent.state,
            crate::state::State::Stopped(crate::state::StopReason::FeedbackRequired { .. })
        ));

        let State::Stopped(StopReason::FeedbackRequired { tool_call, .. }) = agent.state.clone()
        else {
            panic!("Expected feedback required");
        };

        // Step 5: Simulate feedback, run again and assert finish.
        agent
            .context
            .feedback_received(&tool_call, &ToolFeedback::approved())
            .await
            .unwrap();

        tracing::debug!("running after approval");
        agent.run_once().await.unwrap();
        assert!(agent.is_stopped());
    }

    #[test_log::test(tokio::test)]
    async fn test_agent_with_approval_required_tool_denied() {
        use super::*;
        use crate::tools::control::ApprovalRequired;
        use crate::{assistant, chat_request, chat_response, user};
        use swiftide_core::chat_completion::ToolCall;

        // Step 1: Build a tool that needs approval.
        let mock_tool = MockTool::default();

        let approval_tool = ApprovalRequired(mock_tool.boxed());

        // Step 2: Set up the mock LLM.
        let mock_llm = MockChatCompletion::new();

        let chat_req1 = chat_request! {
            user!("Request with approval");
            tools = [approval_tool.clone()]
        };
        let chat_resp1 = chat_response! {
            "Completion message";
            tool_calls = ["mock_tool"]
        };
        mock_llm.expect_complete(chat_req1.clone(), Ok(chat_resp1));

        // The response will include the previous request, but no tool output
        // from the required tool
        let chat_req2 = chat_request! {
            user!("Request with approval"),
            assistant!("Completion message", ["mock_tool"]),
            tool_output!("mock_tool", "This tool call was refused");
            // Simulate feedback required output
            tools = [approval_tool.clone()]
        };
        let chat_resp2 = chat_response! {
            "Post-feedback message";
            tool_calls = ["stop"]
        };
        mock_llm.expect_complete(chat_req2.clone(), Ok(chat_resp2));

        // Step 3: Wire up the agent.
        let mut agent = Agent::builder()
            .tools([approval_tool])
            .llm(&mock_llm)
            .no_system_prompt()
            .build()
            .unwrap();

        // Step 4: Run agent to trigger approval.
        agent.query_once("Request with approval").await.unwrap();

        assert!(matches!(
            agent.state,
            crate::state::State::Stopped(crate::state::StopReason::FeedbackRequired { .. })
        ));

        let State::Stopped(StopReason::FeedbackRequired { tool_call, .. }) = agent.state.clone()
        else {
            panic!("Expected feedback required");
        };

        // Step 5: Simulate feedback, run again and assert finish.
        agent
            .context
            .feedback_received(&tool_call, &ToolFeedback::refused())
            .await
            .unwrap();

        tracing::debug!("running after approval");
        agent.run_once().await.unwrap();

        let history = agent.context().history().await.unwrap();
        history
            .iter()
            .rfind(|m| {
                let ChatMessage::ToolOutput(.., ToolOutput::Text(msg)) = m else {
                    return false;
                };
                msg.contains("refused")
            })
            .expect("Could not find refusal message");

        assert!(agent.is_stopped());
    }

    #[test_log::test(tokio::test)]
    async fn test_removing_default_stop_tool() {
        let mock_llm = MockChatCompletion::new();
        let mock_tool = MockTool::new("mock_tool");

        // Build agent with without_default_stop_tool
        let agent = Agent::builder()
            .without_default_stop_tool()
            .tools([mock_tool.clone()])
            .llm(&mock_llm)
            .no_system_prompt()
            .build()
            .unwrap();

        // Check that "stop" tool is NOT included
        assert!(agent.find_tool_by_name("stop").is_none());
        // Check that our provided tool is still present
        assert!(agent.find_tool_by_name("mock_tool").is_some());
    }
}
