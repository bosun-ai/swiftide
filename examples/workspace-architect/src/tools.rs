//! Read-only tools that let an analyst explore one crate.
//!
//! The tools run their commands through the agent's executor, so they operate in whatever
//! directory the executor is scoped to. Paths must be relative and may not contain `..`.
//! Symlinks are followed, so a link inside the crate can still point elsewhere.

use std::{
    fmt::Write as _,
    path::{Component, Path},
};

use swiftide::{
    chat_completion::{ToolOutput, errors::ToolError},
    traits::{AgentContext, Command, CommandError},
};

/// Lines returned by one `read_file` call.
const READ_WINDOW_LINES: usize = 250;
/// Matches returned by one `grep` call.
const MAX_GREP_MATCHES: usize = 60;
/// Paths returned by one `list_files` call.
const MAX_LISTED_FILES: usize = 300;

// ANCHOR: tools
#[swiftide::tool(
    description = "List the files of the crate (relative paths, build output excluded).",
    param(
        name = "dir",
        description = "Directory relative to the crate root, e.g. `.` or `src/tools`"
    )
)]
pub async fn list_files(context: &dyn AgentContext, dir: &str) -> Result<ToolOutput, ToolError> {
    let Some(dir) = CratePath::parse(dir) else {
        return Ok(CratePath::rejected(dir));
    };
    let files = run(
        context,
        format!(
            "find {} -type f -not -path '*/target/*' -not -path '*/.git/*' | sort",
            dir.quoted()
        ),
    )
    .await?;

    Ok(first_lines(&files, MAX_LISTED_FILES, "files").into())
}

#[swiftide::tool(
    description = "Read a file of the crate, 250 lines at a time.",
    param(name = "path", description = "File path relative to the crate root"),
    param(
        name = "start_line",
        description = "1-based line to start from; null to start at the top"
    )
)]
pub async fn read_file(
    context: &dyn AgentContext,
    path: &str,
    start_line: &Option<usize>,
) -> Result<ToolOutput, ToolError> {
    let Some(path) = CratePath::parse(path) else {
        return Ok(CratePath::rejected(path));
    };
    let content = context
        .executor()
        .exec_cmd(&Command::read_file(path.as_path()))
        .await?
        .into_string_lossy();

    Ok(line_window(&content, start_line.unwrap_or(1)).into())
}

#[swiftide::tool(
    description = "Search the crate's files with an extended regular expression (grep -E).",
    param(name = "pattern", description = "Regex, e.g. `pub (trait|struct) `"),
    param(
        name = "dir",
        description = "Directory relative to the crate root to search in, e.g. `.` or `src`"
    )
)]
pub async fn grep(
    context: &dyn AgentContext,
    pattern: &str,
    dir: &str,
) -> Result<ToolOutput, ToolError> {
    let Some(dir) = CratePath::parse(dir) else {
        return Ok(CratePath::rejected(dir));
    };
    let matches = run(
        context,
        format!(
            "grep -rnIE --exclude-dir=target --exclude-dir=.git -e {} -- {}",
            shell_quote(pattern),
            dir.quoted()
        ),
    )
    .await?;

    if matches.trim().is_empty() {
        return Ok("No matches.".into());
    }
    Ok(first_lines(&matches, MAX_GREP_MATCHES, "matches").into())
}
// ANCHOR_END: tools

/// Runs a shell command; a non-zero exit (e.g. grep without matches) is still a valid answer.
async fn run(context: &dyn AgentContext, command: String) -> Result<String, ToolError> {
    match context.executor().exec_cmd(&Command::shell(command)).await {
        Ok(output) | Err(CommandError::NonZeroExit(output)) => Ok(output.into_string_lossy()),
        Err(error) => Err(error.into()),
    }
}

/// A path the model asked for: relative, without `..`.
///
/// This keeps the model from naming files outside the crate. It does not resolve symlinks, so it
/// is a guard rail for a read-only demo, not a sandbox.
struct CratePath<'a>(&'a Path);

impl<'a> CratePath<'a> {
    fn parse(raw: &'a str) -> Option<Self> {
        let path = Path::new(raw.trim());
        path.components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
            .then_some(Self(path))
    }

    fn rejected(raw: &str) -> ToolOutput {
        ToolOutput::fail(format!(
            "`{raw}` is outside the crate. Use a path relative to the crate root without `..`."
        ))
    }

    fn as_path(&self) -> &Path {
        self.0
    }

    fn quoted(&self) -> String {
        let path = self.0.to_string_lossy();
        shell_quote(if path.is_empty() { "." } else { &path })
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Keeps the first `limit` lines of a listing and says how many were left out.
fn first_lines(text: &str, limit: usize, noun: &str) -> String {
    let total = text.lines().count();
    let mut shown = text.lines().take(limit).collect::<Vec<_>>().join("\n");
    if total > limit {
        let _ = write!(
            shown,
            "\n… {} more {noun}; narrow the directory or pattern.",
            total - limit
        );
    }
    shown
}

/// Returns up to [`READ_WINDOW_LINES`] lines starting at `start_line`, with a pagination hint.
fn line_window(content: &str, start_line: usize) -> String {
    let total = content.lines().count();
    let start = start_line.max(1);
    if total == 0 {
        return "(empty file)".to_string();
    }
    if start > total {
        return format!("The file has only {total} lines.");
    }

    let end = (start + READ_WINDOW_LINES - 1).min(total);
    let mut window = format!("(lines {start}-{end} of {total})\n");
    for line in content.lines().skip(start - 1).take(end + 1 - start) {
        window.push_str(line);
        window.push('\n');
    }
    if end < total {
        let _ = write!(window, "(continue with start_line = {})", end + 1);
    }
    window
}
