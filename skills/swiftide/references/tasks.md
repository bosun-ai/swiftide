# Tasks cheat sheet

Feature flag: `swiftide-tasks` (enabled by `swiftide-agents`). Module: `swiftide::tasks`.
Every snippet is copied from compiled code; the source file is named above each one.

Imports, from `examples/tasks_fanout.rs` and `examples/workspace-architect/src/graph.rs`:

```rust
use swiftide::tasks::{ConcurrencyModel, JoinInput, NodeError, Task, TaskRunOutcome, Transition};
```

```rust
use swiftide::{
    chat_completion::ChatCompletion,
    tasks::{ConcurrencyModel, JoinInput, Task, TaskError, Transition},
    traits::SimplePrompt,
};
```

Also in `swiftide::tasks`: `TaskNode`, `DynNodeId`, `NodeId`, `CurrentNodes`.

## Model

- `Task<Input, Output>` is a graph of nodes. Every node has `Input`, `Output` and `Error` types.
- `task.starts_with(node)` sets the entry node; its `Input` is the task input.
- Each node gets exactly one `register_transition(node, closure_or_target)`, which receives the
  node's output and returns where it goes next.
- `task.transitions_to_finish()` ends the task; its input type is the task `Output`.
- `task.run(input)` returns `Result<TaskRunOutcome<Output>, TaskError>`.

## Registering nodes

| Method | Node |
| --- | --- |
| `register_node(impl TaskNode)` | Your own type, or `Arc<dyn SimplePrompt>` (`Prompt` in, `String` out) |
| `register_node_fn(\|input: &I\| -> Result<O, E>)` | Synchronous function |
| `register_node_async_fn(\|input: &I\| Box::pin(async move { .. }))` | Async function |

All three return a `NodeId<T>`, which is `Copy` and moved into transition closures.

`examples/workspace-architect/src/graph.rs`:

```rust
let discover = task.register_node_fn({
    let progress = progress.clone();
    move |root: &WorkspaceRoot| {
        let members = workspace::discover(root, settings.selection)?;
        progress.step("discover", format_args!("{members} in {root}"));
        Ok::<_, workspace::DiscoveryError>(members)
    }
});

let analyse = task.register_node(CrateAnalyst::new(llm.clone(), progress.clone()));

let join = task.register_node_fn({
    let progress = progress.clone();
    move |summaries: &JoinInput<CrateSummary>| {
        progress.joined(summaries.len());
        architecture_prompt(summaries)
    }
});

// `Arc<dyn SimplePrompt>` is a task node out of the box: `Prompt` in, `String` out.
let render = task.register_node(Arc::new(llm.clone()) as Arc<dyn SimplePrompt>);

let review = task.register_node(ApprovalGate::new(settings.pre_approved, progress.clone()));

let write = task.register_node_async_fn({
    let output = settings.output;
    let progress = progress.clone();
    move |draft: &ArchitectureDraft| {
        let output = output.clone();
        let progress = progress.clone();
        Box::pin(async move {
            let written = draft.write_to(&output).await?;
            progress.step(
                "write",
                format_args!("{} ({} bytes)", written.path.display(), written.bytes),
            );
            Ok::<_, std::io::Error>(written)
        })
    }
});
```

## Implementing `TaskNode`

Use `DynNodeId<Self>` for the node id parameter. `examples/workspace-architect/src/approval.rs`:

```rust
#[async_trait]
impl TaskNode for ApprovalGate {
    type Input = ArchitectureDraft;
    type Output = Review;
    type Error = Infallible;

    async fn evaluate(
        &self,
        _node_id: &DynNodeId<Self>,
        draft: &ArchitectureDraft,
    ) -> Result<Review, Infallible> {
        let mut state = self.state();
        if state.approved {
            state.pending = None;
            self.progress.step("review", "draft approved");
            return Ok(Review::Approved(draft.clone()));
        }

        state.pending = Some(draft.clone());
        self.progress.step(
            "review",
            format_args!(
                "draft ready ({} bytes), waiting for approval",
                draft.as_str().len()
            ),
        );
        Ok(Review::AwaitingApproval)
    }
}
```

`async_trait` is available as `swiftide::reexports::async_trait::async_trait`.

### An agent as a node, with typed output

Build a fresh agent per evaluation so parallel branches share nothing.
`examples/workspace-architect/src/analyst.rs`:

```rust
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
```

The agent it builds (a `StopWithArgs` tool carries the typed report):

```rust
Agent::builder()
    .name(member.name.as_str())
    .llm(&self.llm)
    .context(DefaultContext::from_executor(executor))
    .system_prompt(SYSTEM_PROMPT)
    // Replace the default `stop` tool with one that requires a `CrateReport`.
    .without_default_stop_tool()
    .tools(tools)
    .before_tool(move |_agent, tool_call| {
        analysis.tool_called(tool_call.name(), tool_call.args().unwrap_or_default());
        Box::pin(async { Ok(()) })
    })
    .limit(MAX_TURNS)
    .build()
```

A shared agent behind `Arc<tokio::sync::Mutex<Agent>>` also works (`examples/tasks.rs`,
`BriefingAgent`). For "run this prompt through an agent" without a custom output type, use
`swiftide::agents::TaskAgent`.

## Transitions

Sequential, with a value built from the previous output. `examples/tasks.rs`:

```rust
task.register_transition(brief_prompt_id, move |brief| {
    agent_id.transitions_with(brief)
})?;

// Step 2: use the structured stop payload to drive a final LLM rendering step.
task.register_transition(agent_id, move |decision: BriefingDecision| {
    Transition::next(
        &handoff_prompt_id,
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
```

### Fan out over a list, then join

`Transition::fan_out(&node, first).and(&node, next)...join_with(join.join())`. Each branch must
also transition to `join.join()`. The join node receives `JoinInput<T>` (iterate it, `len()`).
`examples/workspace-architect/src/graph.rs`:

```rust
// One branch per crate, all evaluating the same analyst node with a different input.
task.register_transition(discover, move |members: WorkspaceMembers| {
    let mut crates = members.crates.into_iter();
    let Some(first) = crates.next() else {
        return Transition::error("the workspace has no crates to document");
    };

    crates
        .fold(Transition::fan_out(&analyse, first), |fan_out, member| {
            fan_out.and(&analyse, member)
        })
        .concurrency_model(ConcurrencyModel::Parallel)
        .join_with(join.join())
})?;
```

```rust
// Each branch hands its `CrateSummary` to the join; the join runs once all have arrived.
task.register_transition(analyse, join.join())?;
task.register_transition(join, {
    let progress = progress.clone();
    move |prompt| {
        progress.step("render", "drafting ARCHITECTURE.md");
        render.transitions_with(prompt)
    }
})?;
```

Set a default for the whole task with
`Task::new().with_default_concurrency_model(ConcurrencyModel::Parallel)` (`examples/tasks_fanout.rs`).

### Pause, inspect, resume

A transition can return `Transition::pause()`. On `resume()` the paused node is evaluated again
with the same input, so pause on a cheap node. `examples/workspace-architect/src/graph.rs`:

```rust
task.register_transition(review, move |decision| match decision {
    Review::Approved(draft) => write.transitions_with(draft).into(),
    Review::AwaitingApproval => Transition::pause(),
})?;
```

`examples/workspace-architect/src/main.rs`:

```rust
let mut outcome = task.run(root).await?;
loop {
    match outcome {
        TaskRunOutcome::Completed(written) => return Ok(Some(written)),
        TaskRunOutcome::Paused => {
            if !approve_paused_draft(&task)? {
                return Ok(None);
            }
            outcome = task.resume().await?;
        }
    }
}
```

```rust
fn approve_paused_draft(task: &Task<WorkspaceRoot, WrittenDocument>) -> Result<bool> {
    let CurrentNodes::One(gate) = task.current_nodes::<ApprovalGate>() else {
        bail!("the task paused somewhere other than the approval gate");
    };
```

## Errors

`TaskError::NodeError(failed)` carries the node's own error in `failed.node_error`; surface it
instead of the generic task error. `examples/workspace-architect/src/main.rs`:

```rust
fn headline(error: RunError) -> anyhow::Error {
    match error {
        RunError::Task(TaskError::NodeError(failed)) => {
            anyhow::Error::from_boxed(failed.node_error)
        }
        RunError::Task(other) => other.into(),
        RunError::Approval(other) => other,
    }
}
```
