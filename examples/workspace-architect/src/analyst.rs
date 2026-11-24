//! The fan-out node: one agent per crate, returning a typed [`CrateSummary`].

use std::sync::Arc;

use serde::Deserialize as _;
use swiftide::{
    agents::{
        Agent, AgentBuilderError, DefaultContext, StopReason, errors::AgentError,
        tools::local_executor::LocalExecutor,
    },
    chat_completion::{ChatCompletion, Tool},
    prompt::Prompt,
    reexports::async_trait::async_trait,
    tasks::{DynNodeId, TaskNode},
    traits::ToolExecutor,
};

use crate::{
    progress::{Analysis, Progress},
    summary::{CrateReport, CrateSummary, summary_stop_tool},
    tools::{grep, list_files, read_file},
    workspace::WorkspaceCrate,
};

/// Upper bound on completions per agent query; a thorough analysis needs roughly 10 to 20.
///
/// Swiftide resets the counter on every `query`, so each correction gets a fresh budget and one
/// crate can use up to `(1 + MAX_CORRECTIONS) * MAX_TURNS` completions.
const MAX_TURNS: usize = 30;

const SYSTEM_PROMPT: &str = "\
You are a senior Rust engineer preparing one section of a workspace ARCHITECTURE.md.
Your working directory is the root of ONE crate of the workspace. Explore it with the tools, \
then call `stop` exactly once with your findings: `purpose`, `key_types`, `depends_on` and \
`notes` are its top-level arguments.

How to explore (aim for 10 to 20 tool calls):
1. `list_files` on `.` to see the layout.
2. `read_file` on `Cargo.toml`, then on `src/lib.rs` (or `src/main.rs`). Crate docs and the \
`pub mod` / `pub use` lines reveal the public surface.
3. `grep` for `pub (trait|struct|enum|fn) ` and read the two to four modules that define the \
central abstractions. Skim tests and examples only if the purpose is still unclear.

Writing rules:
- Every statement must be backed by code you read. Copy item names exactly; never guess them.
- Explain responsibilities and how the pieces fit together, not a list of files.
- Prefer items re-exported from the crate root, traits meant to be implemented, and the main \
entry points or builders. Only items reachable from outside the crate (re-exported or in a \
`pub mod`) are public API; call anything in a private module internal, never an extension point. \
Name proc macros the way users write them, e.g. `#[derive(Tool)]` or `#[tool]`, not by their \
implementing function.
- Be concrete and terse. No marketing language.";

// ANCHOR: analyst
/// A task node that documents one workspace crate with a freshly built agent.
///
/// The node is registered once and used by every fan-out branch: each branch evaluates it with a
/// different [`WorkspaceCrate`], and every evaluation builds its own agent scoped to that crate's
/// directory, so branches share nothing and can run in parallel.
#[derive(Clone)]
pub struct CrateAnalyst<L> {
    llm: L,
    progress: Progress,
}

impl<L: ChatCompletion + Clone + 'static> CrateAnalyst<L> {
    pub fn new(llm: L, progress: Progress) -> Self {
        Self { llm, progress }
    }
}
// ANCHOR_END: analyst

impl<L: ChatCompletion + Clone + 'static> CrateAnalyst<L> {
    // ANCHOR: agent
    fn agent_for(
        &self,
        member: &WorkspaceCrate,
        analysis: Analysis,
    ) -> Result<Agent, AgentBuilderError> {
        // Every command the tools run executes inside the crate directory.
        let executor: Arc<dyn ToolExecutor> = Arc::new(LocalExecutor::new(&member.dir));
        let tools: [Box<dyn Tool>; 4] = [
            list_files(),
            read_file(),
            grep(),
            summary_stop_tool().into(),
        ];

        Agent::builder()
            .name(member.name.as_str())
            .llm(&self.llm)
            .context(DefaultContext::from_executor(executor))
            .system_prompt(SYSTEM_PROMPT)
            // Replace the default `stop` tool with one that requires a `CrateReport`.
            .without_default_stop_tool()
            .tools(tools)
            // ANCHOR: hooks
            .before_tool(move |_agent, tool_call| {
                analysis.tool_called(tool_call.name(), tool_call.args().unwrap_or_default());
                Box::pin(async { Ok(()) })
            })
            // ANCHOR_END: hooks
            .limit(MAX_TURNS)
            .build()
    }
    // ANCHOR_END: agent
}

/// The per-crate brief: facts from `cargo metadata` the agent should not have to rediscover.
fn brief(member: &WorkspaceCrate) -> Prompt {
    let deps: Vec<String> = member
        .workspace_deps
        .iter()
        .map(ToString::to_string)
        .collect();

    Prompt::from(
        "Document the crate `{{ name }}`. Your working directory is its root.\n\n\
         Facts from `cargo metadata`:\n\
         - Cargo.toml description (may be inherited from the workspace): {{ description }}\n\
         - Workspace crates it depends on: {% for dep in deps %}{{ dep }}{% if not loop.last %}, \
         {% endif %}{% else %}none{% endfor %}\n\
         - Cargo features: {% for feature in features %}{{ feature }}{% if not loop.last %}, \
         {% endif %}{% else %}none{% endfor %}\n\n\
         Find out what each workspace dependency is used for and fill `depends_on` with exactly \
         those crates. When you are done, call `stop`.",
    )
    .with_context_value("name", member.name.as_str())
    .with_context_value(
        "description",
        member.description.as_deref().unwrap_or("(none)"),
    )
    .with_context_value("deps", deps)
    .with_context_value("features", member.features.as_slice())
}

// ANCHOR: task_node
/// How often an agent is told to fix a missing or malformed report before the crate fails.
const MAX_CORRECTIONS: usize = 2;

#[async_trait]
impl<L: ChatCompletion + Clone + 'static> TaskNode for CrateAnalyst<L> {
    type Input = WorkspaceCrate;
    type Output = CrateSummary;
    type Error = AnalystError;

    async fn evaluate(
        &self,
        _node_id: &DynNodeId<Self>,
        member: &WorkspaceCrate,
    ) -> Result<CrateSummary, AnalystError> {
        self.analyse(member).await.map_err(|cause| AnalystError {
            crate_name: member.name.clone(),
            cause,
        })
    }
}

impl<L: ChatCompletion + Clone + 'static> CrateAnalyst<L> {
    async fn analyse(&self, member: &WorkspaceCrate) -> Result<CrateSummary, AnalysisFailure> {
        let analysis = self.progress.analysis(&member.name);
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

        analysis.finished(&report);
        Ok(report.reconciled_with(member))
    }
}

/// The report the agent passed to `stop`, checked against the `CrateReport` schema.
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
// ANCHOR_END: task_node

/// Analysing one crate failed; the message names the crate.
#[derive(Debug, thiserror::Error)]
#[error("analysing `{crate_name}` failed: {cause}")]
pub struct AnalystError {
    crate_name: String,
    cause: AnalysisFailure,
}

#[derive(Debug, thiserror::Error)]
pub enum AnalysisFailure {
    #[error("could not build the agent: {0}")]
    Build(#[from] AgentBuilderError),
    #[error(transparent)]
    Agent(#[from] AgentError),
    #[error(transparent)]
    Report(#[from] ReportProblem),
}

/// Why the agent's answer is not a usable [`CrateReport`].
#[derive(Debug, thiserror::Error)]
pub enum ReportProblem {
    #[error("the agent did not submit a report: {}", describe(stop_reason.as_ref()))]
    Missing { stop_reason: Option<StopReason> },
    #[error("the `stop` arguments do not match the schema: {0}")]
    Invalid(#[from] serde_json::Error),
}

impl ReportProblem {
    /// What to tell the agent so it can fix its answer.
    fn correction(&self) -> String {
        match self {
            ReportProblem::Missing { .. } => {
                "You have not submitted the summary yet. Call `stop` now \
                                              with the crate summary, based on what you have \
                                              read so far."
                    .to_string()
            }
            ReportProblem::Invalid(error) => format!(
                "Your `stop` arguments do not match the schema: {error}. Call `stop` again with \
                 `purpose`, `key_types`, `depends_on` and `notes` as top-level arguments."
            ),
        }
    }
}

/// A short, human-readable reason why the agent stopped, for progress lines and errors.
fn describe(stop_reason: Option<&StopReason>) -> &'static str {
    match stop_reason {
        None => "it has not stopped",
        Some(StopReason::RequestedByTool(..)) => "it called `stop` without arguments",
        Some(StopReason::NoNewMessages) => "it answered without a tool call or hit the turn limit",
        Some(StopReason::AgentFailed(_)) => "it reported a failure",
        Some(StopReason::ToolCallsOverLimit(_)) => "a tool kept failing",
        Some(StopReason::Error) => "it ran into an error",
        Some(_) => "it stopped for another reason",
    }
}
