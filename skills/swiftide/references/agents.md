# Agents cheat sheet (Swiftide 0.33)

Feature flags: `swiftide-agents` (plus `mcp` for MCP toolboxes, `macros` if default features are
off). Every snippet below is copied from compiled code in the Swiftide repository; the source file
is named above each one.

Imports, from `examples/release-notes-agent/src/agent.rs` and `src/tools.rs`:

```rust
use swiftide::{
    agents::{Agent, DefaultContext, tools::control::ApprovalRequired},
    chat_completion::ChatCompletion,
    prompt::Prompt,
    traits::{ToolBox, ToolExecutor},
};
```

```rust
use swiftide::{
    Tool,
    chat_completion::{Tool, ToolCall, ToolOutput, ToolSpec, errors::ToolError},
    traits::{AgentContext, Command, CommandError, CommandOutput, ToolExecutor},
};
```

The first `Tool` is the derive macro, the second the trait. `StopReason` is
`swiftide::agents::StopReason`, `ToolFeedback` is `swiftide::traits::ToolFeedback`, and
`LocalExecutor` is `swiftide::agents::tools::local_executor::LocalExecutor`.

## Agent loop in one call

`examples/hello_agents.rs`:

```rust
agents::Agent::builder()
    .llm(&openai)
    .tools(vec![search_code(), read_file(), guess_a_number()])
    .before_all(move |_context| {
        // This is a hook that runs before any command is executed
        // No native async closures in Rust yet, so we have to use Box::pin
        Box::pin(async move {
            println!("Hello hook!");
            Ok(())
        })
    })
    // Every message added by the agent will be printed to stdout
    .on_new_message(move |_, msg| {
        let msg = msg.to_string();
        let tx = tx.clone();
        Box::pin(async move {
            tx.send(msg).unwrap();
            Ok(())
        })
    })
    .limit(5)
    .build()?
    .query("In what file can I find an example of a swiftide agent? When you are done guess a number and stop")
    .await?;
```

An agent stops when the model calls `stop`, when a tool asks for feedback, when the model answers
without tool calls, or at `.limit(n)` completions per `query`.

## Tools

### `#[swiftide::tool]` with attribute descriptions

Struct arguments need `Serialize, Deserialize, JsonSchema`. `examples/hello_agents.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct CodeSearchRequest {
    /// Search query to pass to ripgrep
    query: String,
    /// Optional repository root (defaults to the current working directory)
    repo: Option<String>,
    /// Optional list of glob filters for the search
    file_globs: Option<Vec<String>>,
}

#[swiftide::tool(
    description = "Searches code",
    param(name = "request", description = "Code search parameters")
)]
async fn search_code(
    context: &dyn AgentContext,
    request: CodeSearchRequest,
) -> Result<ToolOutput, ToolError> {
    let repo = request.repo.as_deref().unwrap_or(".");
    let mut command = format!("cd {repo} && rg '{query}'", query = request.query);

    if let Some(globs) = &request.file_globs {
        for glob in globs {
            command.push_str(&format!(" -g '{glob}'"));
        }
    }

    let command_output = context
        .executor()
        .exec_cmd(&Command::shell(command))
        .await?;

    Ok(command_output.into())
}
```

Calling `search_code()` returns the tool value to register.

### `#[swiftide::tool]` with doc comments and optional arguments

`examples/release-notes-agent/src/tools.rs`:

```rust
/// Lists every commit since a release tag, oldest first: hash, subject, body and the files it
/// changed.
///
/// # Arguments
/// - `tag`: Tag to start from, e.g. `v0.33.0`. Use null for the latest tag.
#[swiftide::tool]
pub async fn git_log_since(
    context: &dyn AgentContext,
    tag: &Option<String>,
) -> Result<ToolOutput, ToolError> {
    let git = Git::new(context.executor().as_ref());
    let since = match tag {
        Some(tag) => tag.parse()?,
        None => git.latest_tag().await?,
    };
    let log = git.log_since(&since).await?;

    Ok(ToolOutput::text(clip(
        log,
        MAX_LOG_LINES,
        "Use git_diff on a specific path for details.",
    )))
}
```

### `#[derive(Tool)]` for tools with state

The derive calls a method with the snake_case name of the struct. The struct must be `Clone`, and
the crate needs `async-trait` as a dependency. `examples/release-notes-agent/src/tools.rs`:

```rust
#[derive(Clone, Tool)]
#[tool(
    description = "Prepends the finished release notes to the release notes file. A maintainer \
                   reviews the notes before they are written.",
    param(
        name = "markdown",
        description = "The complete release notes for this version, as GitHub flavored Markdown"
    )
)]
pub struct WriteChangelog {
    file: PathBuf,
}

impl WriteChangelog {
    pub fn new(file: impl Into<PathBuf>) -> Self {
        Self { file: file.into() }
    }

    async fn write_changelog(
        &self,
        context: &dyn AgentContext,
        markdown: &str,
    ) -> Result<ToolOutput, ToolError> {
```

The derive also generates `WriteChangelogArgs` (the deserialized arguments), used in the
approval loop below.

### Manual `Tool` with a hand-written schema

`examples/tool_custom_schema.rs`:

```rust
#[swiftide::reexports::async_trait::async_trait]
impl Tool for WorkflowTool {
    async fn invoke(
        &self,
        _agent_context: &dyn AgentContext,
        _tool_call: &ToolCall,
    ) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::text(
            "Workflow execution not implemented in this example",
        ))
    }

    fn name<'tool>(&'tool self) -> Cow<'tool, str> {
        Cow::Borrowed("workflow_tool")
    }

    fn tool_spec(&self) -> ToolSpec {
        ToolSpec::builder()
            .name("workflow_tool")
            .description("Executes a workflow action with strict input choices")
            .parameters_schema(workflow_schema())
            .build()
            .expect("tool spec should be valid")
    }
}
```

### Wrapping a tool (validation before the real call)

Return `ToolOutput::fail(..)` to send an error back to the model without failing the agent.
`examples/release-notes-agent/src/tools.rs`:

```rust
#[derive(Clone)]
pub struct OneEntryPerChange(Box<dyn Tool>);

#[async_trait]
impl Tool for OneEntryPerChange {
    async fn invoke(
        &self,
        context: &dyn AgentContext,
        tool_call: &ToolCall,
    ) -> Result<ToolOutput, ToolError> {
        let args = tool_call
            .args()
            .ok_or_else(|| ToolError::missing_arguments(WRITE_CHANGELOG))?;
        let draft: WriteChangelogArgs = serde_json::from_str(args)?;

        let repeated = repeated_references(&draft.markdown);
        if repeated.is_empty() {
            return self.0.invoke(context, tool_call).await;
        }
        let problems = repeated
            .iter()
            .map(|(reference, entries)| format!("{reference} appears in {entries} entries"))
            .collect::<Vec<_>>()
            .join(", ");
        Ok(ToolOutput::fail(format!(
            "{problems}. Each change gets exactly one entry: merge them, then call \
             {WRITE_CHANGELOG} again."
        )))
    }

    fn name(&self) -> Cow<'_, str> {
        self.0.name()
    }

    fn tool_spec(&self) -> ToolSpec {
        self.0.tool_spec()
    }
}

impl From<OneEntryPerChange> for Box<dyn Tool> {
    fn from(tool: OneEntryPerChange) -> Self {
        Box::new(tool)
    }
}
```

`ToolOutput` constructors: `text`, `fail`, `stop`, `agent_failed`. Variants seen in hooks:
`Text`, `Fail`, `Stop`, `FeedbackRequired`.

## Executors

Tools reach the outside world through `context.executor()` and `Command::shell`,
`Command::read_file`, `Command::write_file`. Scope them to a directory with `LocalExecutor`.
`examples/release-notes-agent/src/main.rs`:

```rust
let executor: Arc<dyn ToolExecutor> = Arc::new(
    LocalExecutor::builder()
        .workdir(&root)
        .default_timeout(Some(Duration::from_mins(1)))
        .build()?,
);
```

Pass it with `.context(DefaultContext::from_executor(executor))`. Shorter form, from
`examples/workspace-architect/src/analyst.rs`:
`Arc::new(LocalExecutor::new(&member.dir))`.

## Hooks

Hooks: `before_all`, `on_start`, `before_completion`, `after_completion`, `before_tool`,
`after_tool`, `after_each`, `on_new_message`, `on_stream`, `on_stop`. All return
`Box::pin(async { Ok(()) })`. `examples/release-notes-agent/src/progress.rs`:

```rust
pub fn report_tool_calls(builder: &mut AgentBuilder) {
    builder
        .before_tool(|_agent, tool_call| {
            println!("  → {}", describe_call(tool_call));
            Box::pin(async { Ok(()) })
        })
        .after_tool(|_agent, tool_call, result| {
            println!("  {} {}", tool_call.name(), describe_result(result));
            Box::pin(async { Ok(()) })
        });
}
```

### Streaming

Registering `on_stream` switches the agent to streaming completions. Set `.stream_full(false)` on
the OpenAI client to receive only deltas. `examples/streaming_agents.rs`:

```rust
agents::Agent::builder()
    .llm(&openai)
    .on_stream(|_agent, response| {
        // We print the message chunk if it exists. Streamed responses also include
        // the full response (without tool calls) in `message` and an `id` to map them to
        // previous chunks for convenience.
        //
        // The agent uses the full assembled response at the end of the stream.
        if let Some(delta) = &response.delta {
            print!(
                "{}",
                delta
                    .message_chunk
                    .as_deref()
                    .map(str::to_string)
                    .unwrap_or_default()
            );
        };
        // If `stream_full` is disabled, response.message() will be the accumulated response
        // response.message()

        Box::pin(async move { Ok(()) })
    })
```

## Structured output with `StopWithArgs`

`examples/release-notes-agent/src/notes.rs`:

```rust
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ReleaseNotes {
    /// The version being released, without a leading `v`.
    pub version: String,
    /// One or two sentences on what this release is about, written for users.
    pub summary: String,
    /// New features and notable improvements.
    pub highlights: Vec<Item>,
    /// Changes that require users to update their code. Empty if there are none.
    pub breaking_changes: Vec<Item>,
    /// Bug fixes users may notice.
    pub fixes: Vec<Item>,
}

impl ReleaseNotes {
    /// A `stop` tool whose arguments must match [`ReleaseNotes`].
    ///
    /// With a custom schema the arguments are forwarded as-is, and end up in
    /// `StopReason::RequestedByTool`.
    pub fn stop_tool() -> StopWithArgs {
        StopWithArgs::with_parameters_schema(schema_for!(ReleaseNotes))
    }
}
```

Reading the payload, with a retry when the model gets it wrong.
`examples/workspace-architect/src/analyst.rs`:

```rust
fn submitted_report(agent: &Agent) -> Result<CrateReport, ReportProblem> {
    match agent.stop_reason() {
        Some(StopReason::RequestedByTool(_, Some(arguments))) => {
            Ok(CrateReport::deserialize(arguments)?)
        }
        other => Err(ReportProblem::Missing {
            stop_reason: other.cloned(),
        }),
    }
}
```

```rust
let mut agent = self.agent_for(member, analysis.clone())?;
agent.query(brief(member)).await?;

// Models occasionally answer in prose or nest the `stop` arguments; say what went wrong.
let mut corrections = 0;
let report = loop {
    match submitted_report(&agent) {
        Ok(report) => break report,
        Err(problem) if corrections < MAX_CORRECTIONS => {
            corrections += 1;
            analysis.corrected(&problem);
            agent.query(problem.correction()).await?;
        }
        Err(problem) => return Err(problem.into()),
    }
};
```

`AgentCanFail::with_parameters_schema` is the same pattern for a structured failure
(`examples/agent_can_fail_custom_schema.rs`). For a single structured LLM call without an agent,
use `client.structured_prompt::<T>(prompt)` (`examples/structured_prompt.rs`).

## Human approval

`ApprovalRequired::new(tool)` makes the agent stop with `StopReason::FeedbackRequired` before the
tool runs. Approve, or refuse and follow up with feedback.
`examples/release-notes-agent/src/session.rs`:

```rust
let tool_call = match agent.stop_reason() {
    Some(StopReason::FeedbackRequired { tool_call, .. }) => tool_call.clone(),
```

```rust
match reviewer.review(&proposal)? {
    Verdict::Approve => {
        agent
            .context()
            .feedback_received(&tool_call, &ToolFeedback::approved())
            .await?;
        // Resumes the pending tool call, which now runs, and continues the loop.
        agent.run().await?;
        // The write itself can still fail; the agent then hears why and carries on.
        written |= succeeded(agent, &tool_call).await?;
    }
    Verdict::Revise(feedback) => {
        agent
            .context()
            .feedback_received(&tool_call, &ToolFeedback::refused())
            .await?;
        // The refusal is recorded as the tool's output, followed by the feedback.
        agent.query(feedback).await?;
    }
    Verdict::Reject => return Ok(Outcome::Rejected),
}
```

A minimal version is in `examples/agents_with_human_in_the_loop.rs`. Custom approval flows can
return `ToolOutput::FeedbackRequired(..)` from a tool directly.

## MCP toolboxes

Feature `mcp`, plus `rmcp` with `client` and `transport-child-process`.
`examples/release-notes-agent/src/mcp.rs`:

```rust
let server = Command::new("npx").configure(|cmd| {
    cmd.args(["-y", "@modelcontextprotocol/server-github"])
        .env("GITHUB_PERSONAL_ACCESS_TOKEN", token);
});
let (transport, _stderr) = TokioChildProcess::builder(server)
    .stderr(Stdio::null())
    .spawn()
    .context("could not run `npx`; is Node.js installed?")?;

let mut toolbox = timeout(
    Duration::from_mins(1),
    McpToolbox::try_from_transport(transport),
)
.await
.context("the GitHub MCP server did not start within a minute")?
.context("the GitHub MCP server failed to start")?;
toolbox.with_whitelist(GITHUB_TOOLS);
// Swiftide names MCP tools `<server>:<tool>`; OpenAI rejects ':', see `OpenAiToolNames` below.
let toolbox = OpenAiToolNames(toolbox);

// Fail early, rather than mid-run, if the server does not offer the tools we expect.
let available = toolbox.available_tools().await?;
```

The renaming wrapper is a `ToolBox` around the `McpToolbox`:

```rust
#[async_trait]
impl<T: ToolBox + Clone> ToolBox for OpenAiToolNames<T> {
    async fn available_tools(&self) -> Result<Vec<Box<dyn Tool>>> {
        let tools = self.0.available_tools().await?;
        Ok(tools
            .into_iter()
            .map(|tool| Box::new(RenamedTool::new(tool)) as Box<dyn Tool>)
            .collect())
    }

    fn name(&self) -> Cow<'_, str> {
        self.0.name()
    }
}
```

Alternative from an already running `rmcp` service (`examples/agents_mcp_tools.rs`):

```rust
let everything_toolbox = McpToolbox::from_running_service(running_service)
    .with_whitelist(["add"])
    .to_owned();
```

Register with `builder.add_toolbox(toolbox)`. Tools are listed when the agent first starts.

## Resuming from stored history

`examples/agents_resume.rs`:

```rust
let stored_history = serde_json::to_string(&first_agent.history().await?)?;
let retrieved_history: Vec<_> = serde_json::from_str(&stored_history)?;

let restored_context = DefaultContext::default()
    .with_existing_messages(retrieved_history)
    .await?
    .to_owned();
```

```rust
let mut second_agent = agents::Agent::builder()
    .llm(&openai)
    .context(restored_context)
    // We'll use the one from the first agent, alternatively we could also pop it from the
    // previous history and add a new one here
    .no_system_prompt()
    .build()?;
```

## Testing agents offline

Implement `ChatCompletion` on a scripted type and pass it to `.llm(..)`. See the `ScriptedLlm` test
doubles in `examples/release-notes-agent/src/session.rs` and
`examples/workspace-architect/src/graph.rs`.
