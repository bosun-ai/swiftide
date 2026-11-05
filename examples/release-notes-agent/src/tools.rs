//! The tools the release notes agent works with.
//!
//! All of them run through the agent context's executor, a `LocalExecutor` whose working directory
//! is the repository root. LLM-provided arguments are parsed into [`Revision`] and [`RepoPath`]
//! first, so a tool call cannot leave the repository or inject shell.
use std::{
    borrow::Cow,
    fmt::Write as _,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use swiftide::{
    Tool,
    chat_completion::{Tool, ToolCall, ToolOutput, ToolSpec, errors::ToolError},
    traits::{AgentContext, Command, CommandError, CommandOutput, ToolExecutor},
};

use crate::{
    git::{Git, RepoPath, Revision, shell_quote},
    notes::{prepend_notes, repeated_references},
};

/// Name of the [`WriteChangelog`] tool, as the model sees it.
pub const WRITE_CHANGELOG: &str = "write_changelog";

const MAX_LOG_LINES: usize = 800;
const MAX_DIFF_LINES: usize = 400;
const MAX_FILE_LINES: usize = 400;

// ANCHOR: tools
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

/// Shows the diff of one file or directory over a revision range.
///
/// # Arguments
/// - `range`: Revision range, e.g. `v0.33.0..HEAD`, or a single commit hash.
/// - `path`: File or directory relative to the repository root.
#[swiftide::tool]
pub async fn git_diff(
    context: &dyn AgentContext,
    range: &str,
    path: &str,
) -> Result<ToolOutput, ToolError> {
    let (range, path): (Revision, RepoPath) = (range.parse()?, path.parse()?);
    let diff = Git::new(context.executor().as_ref())
        .diff(&range, &path)
        .await?;
    if diff.trim().is_empty() {
        return Ok(ToolOutput::text(format!(
            "No changes to {path} in {range}."
        )));
    }

    Ok(ToolOutput::text(clip(
        diff,
        MAX_DIFF_LINES,
        "Diff a narrower path to see the rest.",
    )))
}

/// Reads a file from the repository, e.g. to check a public API or the style of the changelog.
///
/// # Arguments
/// - `path`: File path relative to the repository root.
#[swiftide::tool]
pub async fn read_file(context: &dyn AgentContext, path: &str) -> Result<ToolOutput, ToolError> {
    let path: RepoPath = path.parse()?;
    let content = context
        .executor()
        .exec_cmd(&Command::read_file(path.as_path()))
        .await?;

    Ok(ToolOutput::text(clip(
        content.into_string_lossy(),
        MAX_FILE_LINES,
        "The file is longer than shown.",
    )))
}
// ANCHOR_END: tools

// ANCHOR: write_changelog
/// Prepends the approved release notes to the release notes file.
///
/// The file is fixed by the CLI (`--output`), never chosen by the model. Wrap this tool in
/// `ApprovalRequired` so a human signs off before anything is written.
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
        let executor = context.executor();
        // A missing file is started; any other failure must not overwrite the existing notes.
        let existing = if file_exists(executor.as_ref(), &self.file).await? {
            executor
                .exec_cmd(&Command::read_file(&self.file))
                .await
                .map(CommandOutput::into_string_lossy)?
        } else {
            String::new()
        };

        executor
            .exec_cmd(&Command::write_file(
                &self.file,
                prepend_notes(markdown, &existing),
            ))
            .await?;

        Ok(ToolOutput::text(format!("wrote {}", self.file.display())))
    }
}
// ANCHOR_END: write_changelog

async fn file_exists(executor: &dyn ToolExecutor, file: &Path) -> Result<bool, ToolError> {
    let test = Command::shell(format!("test -e {}", shell_quote(&file.to_string_lossy())));
    match executor.exec_cmd(&test).await {
        Ok(_) => Ok(true),
        Err(CommandError::NonZeroExit(_)) => Ok(false),
        Err(err) => Err(err.into()),
    }
}

/// Sends a `write_changelog` draft back to the model, before a maintainer sees it, when one pull
/// request or commit is described in more than one entry.
#[derive(Clone)]
pub struct OneEntryPerChange(Box<dyn Tool>);

impl OneEntryPerChange {
    pub fn new(write_changelog: impl Tool + 'static) -> Self {
        Self(Box::new(write_changelog))
    }
}

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

/// Keeps tool output within a budget the model can use, telling it what was left out.
fn clip(mut text: String, max_lines: usize, hint: &str) -> String {
    let Some((cut, _)) = text.match_indices('\n').nth(max_lines.saturating_sub(1)) else {
        return text;
    };
    let omitted = text[cut + 1..].lines().count();
    if omitted == 0 {
        return text;
    }
    text.truncate(cut);
    let _ = write!(text, "\n\n[... {omitted} more lines omitted. {hint}]");
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_changelog_name_matches_the_tool() {
        use swiftide::chat_completion::Tool as _;
        assert_eq!(WriteChangelog::new("NOTES.md").name(), WRITE_CHANGELOG);
    }

    #[test]
    fn clip_keeps_short_output() {
        assert_eq!(clip("a\nb\n".into(), 2, "hint"), "a\nb\n");
    }

    #[test]
    fn clip_reports_what_it_dropped() {
        assert_eq!(
            clip("a\nb\nc\nd".into(), 2, "Narrow it."),
            "a\nb\n\n[... 2 more lines omitted. Narrow it.]"
        );
    }
}
