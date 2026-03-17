//! This example shows what tasks are good at: wiring a few typed steps into a small workflow.
//!
//! The flow is intentionally split into two kinds of nodes:
//! - a prompt-model node that uses the built-in task convenience helpers
//! - a custom agent node that implements `TaskNode` directly and returns structured output
//!
//! That is the usual split in real applications:
//! - use the convenience adapters when a Swiftide primitive already fits the task shape
//! - implement `TaskNode` yourself when the step has domain-specific behavior

use std::sync::Arc;

use anyhow::Result;
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use swiftide::{
    agents::{
        self, Agent, StopReason,
        errors::AgentError,
        tasks::{NodeId, Task, TaskNode, TaskRunState},
        tools::control::StopWithArgs,
    },
    prompt::Prompt,
    traits::SimplePrompt,
};
use tokio::sync::Mutex;

/// A domain-specific task step around an `Agent`.
///
/// We do not use `TaskAgent` here on purpose. `TaskAgent` is useful when you simply want
/// "run this prompt through an agent" behavior. In many applications, however, an agent step has
/// its own responsibility and its own input/output shape. That is when implementing `TaskNode`
/// directly becomes the clearer API.
#[derive(Clone, Debug)]
struct BriefingAgent {
    agent: Arc<Mutex<Agent>>,
}

impl BriefingAgent {
    fn new(agent: Agent) -> Self {
        Self {
            agent: Arc::new(Mutex::new(agent)),
        }
    }
}

/// The structured payload that our custom stop tool expects from the agent.
///
/// Returning a typed value like this is the main reason to implement a task node around an agent:
/// downstream task steps can now work with domain data instead of unstructured free text.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
struct BriefingDecision {
    audience: String,
    summary: String,
    next_steps: Vec<String>,
}

#[derive(Debug)]
enum BriefingAgentError {
    Agent(AgentError),
    MissingStructuredStop,
    InvalidStructuredStop(serde_json::Error),
}

impl std::fmt::Display for BriefingAgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Agent(error) => write!(f, "{error}"),
            Self::MissingStructuredStop => {
                write!(f, "agent stopped without the structured stop payload")
            }
            Self::InvalidStructuredStop(error) => {
                write!(
                    f,
                    "agent returned an invalid structured stop payload: {error}"
                )
            }
        }
    }
}

impl std::error::Error for BriefingAgentError {}

impl From<AgentError> for BriefingAgentError {
    fn from(value: AgentError) -> Self {
        Self::Agent(value)
    }
}

impl From<serde_json::Error> for BriefingAgentError {
    fn from(value: serde_json::Error) -> Self {
        Self::InvalidStructuredStop(value)
    }
}

#[swiftide::reexports::async_trait::async_trait]
impl TaskNode for BriefingAgent {
    type Input = String;
    type Output = BriefingDecision;
    type Error = BriefingAgentError;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        brief: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        // The previous node produced a short brief. This node asks the agent to turn that brief
        // into a structured hand-off plan and to stop with a JSON payload we can deserialize.
        let handoff_prompt = Prompt::from(
            "Turn this brief into a structured hand-off plan.\n\
             Brief:\n\
             {{brief}}\n\n\
             Call the `stop` tool once you have produced the final plan.",
        )
        .with_context_value("brief", brief.clone());

        let mut agent = self.agent.lock().await;
        agent.query_once(handoff_prompt).await?;

        let Some(StopReason::RequestedByTool(_, Some(payload))) = agent.stop_reason() else {
            return Err(BriefingAgentError::MissingStructuredStop);
        };

        Ok(serde_json::from_value(payload.clone())?)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let openai = swiftide::integrations::openai::OpenAI::builder()
        .default_embed_model("text-embeddings-3-small")
        .default_prompt_model("gpt-4o-mini")
        .build()?;

    let stop_tool = StopWithArgs::with_parameters_schema(schema_for!(BriefingDecision));

    let agent = agents::Agent::builder()
        .llm(&openai)
        .without_default_stop_tool()
        .tools([stop_tool])
        .system_prompt(
            "You are an operations lead.\n\
             Convert briefs into a concise hand-off decision.\n\
             When you are done, call the `stop` tool with the provided JSON schema.\n\
             Fill `next_steps` with concrete follow-up actions.",
        )
        .build()?;

    // Tasks are typed from input to output. This task starts with a `Prompt` and finishes with the
    // final teammate-facing hand-off note.
    let mut task: Task<Prompt, String> = Task::new();

    // Convenience helper: `Arc<dyn SimplePrompt>` already implements `TaskNode`, so prompt-model
    // steps can be registered directly without any wrapper code.
    let prompt_step: Arc<dyn SimplePrompt> = Arc::new(openai.clone());
    let brief_prompt_id = task.register_node(prompt_step.clone());
    let handoff_prompt_id = task.register_node(prompt_step);

    // Custom node: the agent step owns the stop-tool contract and returns structured task data.
    let agent_id = task.register_node(BriefingAgent::new(agent));

    task.starts_with(brief_prompt_id);

    // Step 1: turn the incoming request into a short brief that the agent can work from.
    task.register_transition(brief_prompt_id, move |brief| {
        agent_id.transitions_with(brief)
    })?;

    // Step 2: use the structured stop payload to drive a final LLM rendering step.
    task.register_transition(agent_id, move |decision: BriefingDecision| {
        handoff_prompt_id.transitions_with(
            Prompt::from(
                "Write a teammate-facing hand-off note.\n\
                 Audience: {{audience}}\n\
                 Summary: {{summary}}\n\
                 Next steps:\n\
                 {% for step in next_steps %}- {{ step }}\n{% endfor %}",
            )
            .with_context_value("audience", decision.audience)
            .with_context_value("summary", decision.summary)
            .with_context_value("next_steps", decision.next_steps),
        )
    })?;

    // Step 3: the rendered hand-off note becomes the task output.
    task.register_transition(handoff_prompt_id, task.transitions_to_finish())?;

    // The input prompt is still a normal Swiftide `Prompt`, so it can use templating and context.
    let request = Prompt::from("Create a short operations brief about this topic: {{topic}}")
        .with_context_value(
            "topic",
            "rolling out the new task runtime to internal teams",
        );

    match task.run(request).await? {
        TaskRunState::Completed(handoff_note) => {
            println!("Generated hand-off note:\n{handoff_note}");
        }
        TaskRunState::Paused => {
            println!("Task paused");
        }
    }

    Ok(())
}
