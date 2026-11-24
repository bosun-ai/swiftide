//! Wiring the nodes into a task graph.
//!
//! ```text
//! discover ──fan-out──▶ analyse × N (parallel) ──▶ join (builds prompt) ──▶ render ──▶ review ──▶ write
//!                                                                                      │  ▲
//!                                                                             pause ◀──┘  └── resume
//! ```

use std::{path::PathBuf, sync::Arc};

use swiftide::{
    chat_completion::ChatCompletion,
    tasks::{ConcurrencyModel, JoinInput, Task, TaskError, Transition},
    traits::SimplePrompt,
};

use crate::{
    analyst::CrateAnalyst,
    approval::{ApprovalGate, Review},
    progress::Progress,
    render::{ArchitectureDraft, WrittenDocument, architecture_prompt},
    summary::CrateSummary,
    workspace::{self, MemberSelection, WorkspaceMembers, WorkspaceRoot},
};

/// Choices the CLI makes for one run.
#[derive(Debug, Clone)]
pub struct ArchitectSettings {
    pub selection: MemberSelection,
    pub output: PathBuf,
    pub pre_approved: bool,
}

// ANCHOR: graph
/// Builds the task: `WorkspaceRoot` in, the written ARCHITECTURE.md out.
pub fn architect_task<L>(
    llm: &L,
    settings: ArchitectSettings,
    progress: &Progress,
) -> Result<Task<WorkspaceRoot, WrittenDocument>, TaskError>
where
    L: ChatCompletion + SimplePrompt + Clone + 'static,
{
    let mut task = Task::new();

    // ANCHOR: nodes
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
    // ANCHOR_END: nodes

    task.starts_with(discover);

    // ANCHOR: fan_out
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
    // ANCHOR_END: fan_out

    // ANCHOR: join
    // Each branch hands its `CrateSummary` to the join; the join runs once all have arrived.
    task.register_transition(analyse, join.join())?;
    task.register_transition(join, {
        let progress = progress.clone();
        move |prompt| {
            progress.step("render", "drafting ARCHITECTURE.md");
            render.transitions_with(prompt)
        }
    })?;
    // ANCHOR_END: join

    task.register_transition(render, move |markdown: String| {
        review.transitions_with(ArchitectureDraft::from_model_output(&markdown))
    })?;

    // ANCHOR: pause
    task.register_transition(review, move |decision| match decision {
        Review::Approved(draft) => write.transitions_with(draft).into(),
        Review::AwaitingApproval => Transition::pause(),
    })?;
    // ANCHOR_END: pause

    task.register_transition(write, task.transitions_to_finish())?;

    Ok(task)
}
// ANCHOR_END: graph

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;
    use swiftide::{
        chat_completion::{
            ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ToolCall,
            errors::LanguageModelError,
        },
        prompt::Prompt,
        reexports::async_trait::async_trait,
        tasks::{CurrentNodes, TaskRunOutcome},
    };

    use super::*;

    /// Plays the model offline: explore once, then stop with a summary; render a fenced document.
    #[derive(Debug, Clone)]
    struct ScriptedLlm;

    #[async_trait]
    impl ChatCompletion for ScriptedLlm {
        async fn complete(
            &self,
            request: &ChatCompletionRequest<'_>,
        ) -> Result<ChatCompletionResponse, LanguageModelError> {
            let explored = request
                .messages()
                .iter()
                .any(|message| matches!(message, ChatMessage::ToolOutput(..)));
            let (name, args) = if explored {
                let report = json!({
                    "purpose": "Does things.",
                    "key_types": ["`Thing`: the thing"],
                    "depends_on": [{ "crate_name": "not-a-dependency", "usage": "invented" }],
                    "notes": "None."
                });
                ("stop", report)
            } else {
                ("list_files", json!({ "dir": "." }))
            };
            let call = ToolCall::builder()
                .id("call")
                .name(name)
                .args(args.to_string())
                .build()
                .expect("valid tool call");

            Ok(ChatCompletionResponse {
                tool_calls: Some(vec![call]),
                ..ChatCompletionResponse::default()
            })
        }
    }

    #[async_trait]
    impl SimplePrompt for ScriptedLlm {
        async fn prompt(&self, prompt: Prompt) -> Result<String, LanguageModelError> {
            let rendered = prompt.render().expect("template renders");
            let graph = rendered
                .split("```mermaid")
                .nth(1)
                .and_then(|rest| rest.split("```").next())
                .unwrap_or_default();
            Ok(format!(
                "```markdown\n# Architecture\n```mermaid{graph}```\n```"
            ))
        }
    }

    #[tokio::test]
    async fn pauses_for_approval_and_writes_after_resume() {
        let root = WorkspaceRoot::locate(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")))
            .expect("workspace root");
        let output =
            std::env::temp_dir().join(format!("workspace-architect-{}.md", std::process::id()));
        let settings = ArchitectSettings {
            selection: MemberSelection::Published,
            output: output.clone(),
            pre_approved: false,
        };
        let mut task = architect_task(&ScriptedLlm, settings, &Progress::new(false)).unwrap();

        let outcome = task.run(root).await.unwrap();
        assert!(matches!(outcome, TaskRunOutcome::Paused));
        assert!(!output.exists(), "nothing is written before approval");

        let CurrentNodes::One(gate) = task.current_nodes::<ApprovalGate>() else {
            panic!("the task should pause at the approval gate");
        };
        let draft = gate.pending_draft().expect("a draft waits for review");
        assert!(
            draft.as_str().starts_with("# Architecture"),
            "fence removed"
        );
        assert!(
            draft
                .as_str()
                .contains("swiftide_agents --> swiftide_tasks")
        );
        assert!(draft.as_str().contains("swiftide -.-> swiftide_agents"));
        gate.approve();

        let TaskRunOutcome::Completed(written) = task.resume().await.unwrap() else {
            panic!("the task should complete after approval");
        };
        let content = std::fs::read_to_string(&written.path).unwrap();
        std::fs::remove_file(&written.path).unwrap();
        assert_eq!(content, draft.as_str());
    }
}
