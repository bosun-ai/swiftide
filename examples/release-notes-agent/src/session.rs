//! Runs the agent until a maintainer has approved the release notes.
use anyhow::{Context as _, Result, bail};
use serde::Deserialize as _;
use swiftide::{
    agents::{Agent, StopReason},
    chat_completion::{ChatMessage, ToolCall, ToolOutput},
    prompt::Prompt,
    traits::ToolFeedback,
};

use crate::{agent::STEP_LIMIT, notes::ReleaseNotes, tools::WriteChangelogArgs};

/// What a maintainer decides about proposed release notes.
pub enum Verdict {
    /// Write them.
    Approve,
    /// Do not write them; ask the agent for another draft with this feedback.
    Revise(String),
    /// Do not write them and stop.
    Reject,
}

/// Someone who signs off on release notes before they are written.
pub trait Reviewer {
    fn review(&mut self, proposal: &WriteChangelogArgs) -> Result<Verdict>;
}

pub enum Outcome {
    /// An approved `write_changelog` call wrote the notes.
    Written {
        /// The structured version of the notes, if the agent stopped with one.
        notes: Option<ReleaseNotes>,
    },
    Rejected,
}

/// Sent once when the model ends its turn without calling a tool.
const KEEP_GOING: &str = "Continue: finish the notes and call `write_changelog` with them.";

// ANCHOR: approval
/// Asks the agent for release notes and loops through review rounds until they are written or
/// rejected.
pub async fn draft_release_notes(
    agent: &mut Agent,
    task: Prompt,
    reviewer: &mut impl Reviewer,
) -> Result<Outcome> {
    agent.query(task).await?;
    let mut written = false;
    let mut nudged = false;

    loop {
        // `write_changelog` is wrapped in `ApprovalRequired`: the agent stops and waits.
        let tool_call = match agent.stop_reason() {
            Some(StopReason::FeedbackRequired { tool_call, .. }) => tool_call.clone(),
            // The custom `stop` tool carries the structured release notes.
            Some(StopReason::RequestedByTool(_, Some(notes))) if written => {
                let notes = ReleaseNotes::deserialize(notes)
                    .context("the agent stopped with malformed release notes")?;
                return Ok(Outcome::Written { notes: Some(notes) });
            }
            _ if written => return Ok(Outcome::Written { notes: None }),
            // A turn without a tool call, or the step limit.
            Some(StopReason::NoNewMessages) => {
                nudge_once(agent, &mut nudged).await?;
                continue;
            }
            other => bail!("the agent stopped without writing the notes: {other:?}"),
        };

        let proposal: WriteChangelogArgs =
            serde_json::from_str(tool_call.args().unwrap_or_default())?;

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
    }
}
// ANCHOR_END: approval

/// Whether `tool_call` ran and returned a result, rather than an error.
async fn succeeded(agent: &Agent, tool_call: &ToolCall) -> Result<bool> {
    let history = agent.history().await?;
    Ok(history.iter().any(|message| {
        matches!(
            message,
            ChatMessage::ToolOutput(call, ToolOutput::Text(_)) if call.id() == tool_call.id()
        )
    }))
}

/// Asks the agent once to continue when it ended its turn without calling a tool, which models
/// sometimes do right after reading tool results.
///
/// Swiftide reports reaching the step limit with the same `StopReason::NoNewMessages`; that, or
/// a second turn without a tool call, ends the session.
async fn nudge_once(agent: &mut Agent, nudged: &mut bool) -> Result<()> {
    if !ended_turn_without_tool_call(&agent.history().await?) {
        bail!(
            "the agent stopped without writing the notes (no tool call, or the {STEP_LIMIT}-step limit was reached)"
        );
    }
    if *nudged {
        bail!(
            "the agent stopped without writing the notes: it twice ended its turn without calling a tool"
        );
    }
    *nudged = true;
    agent.query(KEEP_GOING).await?;
    Ok(())
}

/// Whether the last turn ended without a tool call, rather than at the step limit.
fn ended_turn_without_tool_call(history: &[ChatMessage]) -> bool {
    let last = history
        .iter()
        .rev()
        .find(|message| !matches!(message, ChatMessage::Reasoning(_)));
    // A reply with only text.
    if let Some(ChatMessage::Assistant(_, tool_calls)) = last {
        return tool_calls.as_ref().is_none_or(Vec::is_empty);
    }
    // An empty reply is not recorded, so the history ends with tool results or the question.
    // Every step of the run did record a reply, so at the limit there are as many replies as steps.
    let since_question = history
        .iter()
        .rposition(|message| {
            matches!(
                message,
                ChatMessage::User(_) | ChatMessage::UserWithParts(_)
            )
        })
        .map_or(history, |question| &history[question..]);
    let steps = since_question
        .iter()
        .filter(|message| matches!(message, ChatMessage::Assistant(..)))
        .count();
    steps < STEP_LIMIT
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    use async_trait::async_trait;
    use swiftide::{
        agents::tools::{local_executor::LocalExecutor, mcp::McpToolbox},
        chat_completion::{
            ChatCompletion, ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ToolCall,
            errors::LanguageModelError,
        },
    };

    use super::*;
    use crate::agent::{self, ReleaseTask};

    /// An LLM that answers with one scripted tool call per completion, and records what it saw.
    #[derive(Clone, Default)]
    struct ScriptedLlm {
        replies: Arc<Mutex<VecDeque<(&'static str, String)>>>,
        seen: Arc<Mutex<Vec<Vec<ChatMessage>>>>,
    }

    impl ScriptedLlm {
        fn new(replies: impl IntoIterator<Item = (&'static str, String)>) -> Self {
            Self {
                replies: Arc::new(Mutex::new(replies.into_iter().collect())),
                seen: Arc::default(),
            }
        }
    }

    #[async_trait]
    impl ChatCompletion for ScriptedLlm {
        async fn complete(
            &self,
            request: &ChatCompletionRequest<'_>,
        ) -> Result<ChatCompletionResponse, LanguageModelError> {
            self.seen.lock().unwrap().push(request.messages().to_vec());
            let (tool, args) = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("script ended");
            // An empty tool name scripts a reply without any tool call.
            if tool.is_empty() {
                return Ok(ChatCompletionResponse::builder().build().unwrap());
            }
            let call = ToolCall::builder()
                .id(format!("call_{}", self.seen.lock().unwrap().len()))
                .name(tool)
                .args(args)
                .build()
                .unwrap();
            Ok(ChatCompletionResponse::builder()
                .tool_calls(vec![call])
                .build()
                .unwrap())
        }
    }

    struct ScriptedReviewer(VecDeque<Verdict>);

    impl Reviewer for ScriptedReviewer {
        fn review(&mut self, _proposal: &WriteChangelogArgs) -> Result<Verdict> {
            Ok(self.0.pop_front().expect("unexpected review"))
        }
    }

    fn draft(markdown: &str) -> (&'static str, String) {
        (
            "write_changelog",
            serde_json::json!({ "markdown": markdown }).to_string(),
        )
    }

    fn release_task() -> ReleaseTask {
        ReleaseTask {
            since: "v0.1.0".parse().unwrap(),
            commit_count: 1,
            next_version: Some("0.2.0".into()),
            notes_file: "NOTES.md".into(),
            github: None,
        }
    }

    fn build_agent(llm: &ScriptedLlm, repo: &std::path::Path, task: &ReleaseTask) -> Agent {
        agent::build(
            llm,
            Arc::new(LocalExecutor::new(repo)),
            task,
            None::<McpToolbox>,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn revises_until_approved_then_writes_and_stops() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(repo.path().join("NOTES.md"), "## 0.1.0\n").unwrap();

        let notes = serde_json::json!({
            "version": "0.2.0",
            "summary": "Shorter.",
            "highlights": [],
            "breaking_changes": [],
            "fixes": [{ "title": "Fixed", "details": "It works.", "references": ["#1"] }],
        });
        let llm = ScriptedLlm::new([
            draft("## 0.2.0\n\nA long first draft."),
            draft("## 0.2.0\n\nShorter."),
            ("stop", notes.to_string()),
        ]);
        let task = release_task();
        let mut agent = build_agent(&llm, repo.path(), &task);
        let mut reviewer = ScriptedReviewer(VecDeque::from([
            Verdict::Revise("Make it shorter.".into()),
            Verdict::Approve,
        ]));

        let outcome = draft_release_notes(&mut agent, task.query().into(), &mut reviewer)
            .await
            .unwrap();

        let Outcome::Written {
            notes: Some(written),
        } = outcome
        else {
            panic!("expected the notes to be written");
        };
        assert_eq!(written.version, "0.2.0");
        assert_eq!(
            std::fs::read_to_string(repo.path().join("NOTES.md")).unwrap(),
            "## 0.2.0\n\nShorter.\n\n## 0.1.0\n"
        );

        // The second draft was requested with the refusal and the reviewer's feedback.
        let seen = llm.seen.lock().unwrap();
        let second = seen[1].iter().map(ToString::to_string).collect::<Vec<_>>();
        assert!(second.iter().any(|m| m.contains("refused")), "{second:?}");
        assert!(
            second.iter().any(|m| m.contains("Make it shorter.")),
            "{second:?}"
        );
    }

    #[tokio::test]
    async fn stopping_without_a_write_is_an_error() {
        let repo = tempfile::tempdir().unwrap();
        let notes = serde_json::json!({
            "version": "0.2.0",
            "summary": "Nothing.",
            "highlights": [],
            "breaking_changes": [],
            "fixes": [],
        });
        let llm = ScriptedLlm::new([("stop", notes.to_string())]);
        let task = release_task();
        let mut agent = build_agent(&llm, repo.path(), &task);

        let result = draft_release_notes(
            &mut agent,
            task.query().into(),
            &mut ScriptedReviewer(VecDeque::new()),
        )
        .await;

        let error = result.err().expect("stopping without writing must fail");
        assert!(error.to_string().contains("without writing"), "{error:#}");
        assert!(!repo.path().join("NOTES.md").exists());
    }

    #[tokio::test]
    async fn a_turn_without_tool_calls_is_nudged_once() {
        let repo = tempfile::tempdir().unwrap();
        let llm = ScriptedLlm::new([
            ("", String::new()),
            draft("## 0.2.0\n\nNotes."),
            ("", String::new()),
        ]);
        let task = release_task();
        let mut agent = build_agent(&llm, repo.path(), &task);
        let mut reviewer = ScriptedReviewer(VecDeque::from([Verdict::Approve]));

        let outcome = draft_release_notes(&mut agent, task.query().into(), &mut reviewer)
            .await
            .unwrap();

        // The write succeeded, so ending with an empty turn instead of `stop` still counts.
        assert!(matches!(outcome, Outcome::Written { notes: None }));
        let seen = llm.seen.lock().unwrap();
        let second = seen[1].iter().map(ToString::to_string).collect::<Vec<_>>();
        assert!(second.iter().any(|m| m.contains(KEEP_GOING)), "{second:?}");
    }

    #[tokio::test]
    async fn a_second_turn_without_tool_calls_is_an_error() {
        let repo = tempfile::tempdir().unwrap();
        let llm = ScriptedLlm::new([("", String::new()), ("", String::new())]);
        let task = release_task();
        let mut agent = build_agent(&llm, repo.path(), &task);

        let result = draft_release_notes(
            &mut agent,
            task.query().into(),
            &mut ScriptedReviewer(VecDeque::new()),
        )
        .await;

        let error = result.err().expect("two empty turns must fail");
        assert!(error.to_string().contains("without writing"), "{error:#}");
        assert!(error.to_string().contains("twice"), "{error:#}");
        assert!(!repo.path().join("NOTES.md").exists());
    }

    #[test]
    fn reaching_the_step_limit_is_not_a_turn_without_tool_calls() {
        let call = ToolCall::builder()
            .id("call")
            .name("git_diff")
            .args("{}")
            .build()
            .unwrap();
        let step = [
            ChatMessage::Assistant(None, Some(vec![call.clone()])),
            ChatMessage::ToolOutput(call, ToolOutput::Text("diff".into())),
        ];
        let history = |steps: usize| {
            std::iter::once(ChatMessage::User("Write the notes.".into()))
                .chain(step.iter().cycle().take(2 * steps).cloned())
                .collect::<Vec<_>>()
        };

        assert!(ended_turn_without_tool_call(&history(STEP_LIMIT - 1)));
        assert!(!ended_turn_without_tool_call(&history(STEP_LIMIT)));
        let mut text_only = history(STEP_LIMIT);
        text_only.push(ChatMessage::Assistant(Some("Done.".into()), None));
        assert!(ended_turn_without_tool_call(&text_only));
    }

    #[tokio::test]
    async fn repeated_pull_requests_go_back_to_the_model_before_review() {
        let repo = tempfile::tempdir().unwrap();
        let llm = ScriptedLlm::new([
            draft("## 0.2.0\n\n- **A**: one. (#1)\n- **B**: two. (#1)\n"),
            draft("## 0.2.0\n\n- **A and B**: one and two. (#1)\n"),
            ("stop", serde_json::json!({
                "version": "0.2.0",
                "summary": "A and B.",
                "highlights": [{ "title": "A and B", "details": "One and two.", "references": ["#1"] }],
                "breaking_changes": [],
                "fixes": [],
            }).to_string()),
        ]);
        let task = release_task();
        let mut agent = build_agent(&llm, repo.path(), &task);
        // Only the second draft reaches the reviewer.
        let mut reviewer = ScriptedReviewer(VecDeque::from([Verdict::Approve]));

        let outcome = draft_release_notes(&mut agent, task.query().into(), &mut reviewer)
            .await
            .unwrap();

        assert!(matches!(outcome, Outcome::Written { notes: Some(_) }));
        let seen = llm.seen.lock().unwrap();
        let second = seen[1].iter().map(ToString::to_string).collect::<Vec<_>>();
        assert!(
            second.iter().any(|m| m.contains("#1 appears in 2 entries")),
            "{second:?}"
        );
    }
}
