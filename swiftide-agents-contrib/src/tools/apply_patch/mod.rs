//! File-oriented patch parsing, matching, application, and result reporting.
use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, bail};
use async_trait::async_trait;
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use shell_escape::unix::escape;
use swiftide_core::{
    AgentContext, Command,
    chat_completion::{Tool, ToolCall, ToolOutput, ToolSpecBuilder, errors::ToolError},
};

/// Apply Fluyt's file-oriented patch format with its original parser, matching, and messages.
///
/// Operations run in order and can partially succeed. Inspect the returned report. Paths must be
/// relative and contain no parent-directory components. This check only examines path components;
/// filesystem operations follow symlinks. The executor controls filesystem access, and
/// `LocalExecutor` does not isolate it. Use an isolated executor for untrusted repositories.
///
/// # Examples
///
/// ```no_run
/// use swiftide_agents::Agent;
/// use swiftide_agents_contrib::tools::ApplyPatch;
/// Agent::builder().add_tool(ApplyPatch);
/// ```
#[derive(Debug, Default, Clone)]
pub struct ApplyPatch;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ApplyPatchArgsPayload {
    /// File-oriented patch text in the format described by this tool.
    patch: String,
}

#[derive(Debug, Default)]
struct ApplyPatchSummary {
    operations: Vec<OperationOutcome>,
}

const TOOL_DESCRIPTION: &str = r#"Use the `apply_patch` tool to edit files by passing patch text in the `patch` argument.
The patch format is a file-oriented diff enclosed by these markers:

*** Begin Patch
[ one or more file operations ]
*** End Patch

Each operation starts with one of these headers:

*** Add File: <path> - create a file. Prefix every content line with `+`.
*** Delete File: <path> - delete a file. No content follows.
*** Update File: <path> - update a file, optionally followed by `*** Move to: <new path>`.

An update contains one or more hunks. Start each hunk with `@@`, optionally followed by a class, function, or other anchor. Prefix lines removed from the file with `-`, added lines with `+`, and unchanged context lines with a space. Blank context lines may be left blank or prefixed with a space.

Use about three unchanged lines above and below a change. If that is not unique, add a descriptive `@@` anchor. You can use consecutive anchors to narrow the location further:

@@ class BaseClass
@@ def method():
 unchanged context
-old line
+new line
 unchanged context

A patch may contain several file operations:

*** Begin Patch
*** Add File: hello.txt
+Hello world
*** Update File: src/app.py
*** Move to: src/main.py
@@ def greet():
-print("Hi")
+print("Hello, world!")
*** Delete File: obsolete.txt
*** End Patch

Use repository-relative paths only. Include an Add, Delete, or Update header for every operation, and prefix every line in a new file with `+`."#;

// TODO: This can be derived
#[async_trait]
impl Tool for ApplyPatch {
    async fn invoke(
        &self,
        agent_context: &dyn AgentContext,
        tool_call: &ToolCall,
    ) -> Result<ToolOutput, ToolError> {
        let args_json = tool_call
            .args()
            .ok_or_else(|| ToolError::missing_arguments("apply_patch"))?;
        let payload: ApplyPatchArgsPayload = serde_json::from_str(args_json)?;

        let parsed = parse_patch(&payload.patch).map_err(|err| {
            ToolError::unknown(anyhow!(
                "Patch not applied (0 operations executed): invalid patch: {err}"
            ))
        })?;

        if parsed.hunks.is_empty() {
            return Err(ToolError::unknown(anyhow!(
                "patch must contain at least one operation"
            )));
        }

        let executor = agent_context.executor();
        let mut summary = ApplyPatchSummary::default();

        for hunk in parsed.hunks {
            summary.operations.push(apply_hunk(executor, hunk).await);
        }

        Ok(ToolOutput::text(summary.render()))
    }

    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("apply_patch")
    }

    fn tool_spec(&self) -> swiftide_core::chat_completion::ToolSpec {
        ToolSpecBuilder::default()
            .name("apply_patch")
            .description(TOOL_DESCRIPTION)
            .parameters_schema(schema_for!(ApplyPatchArgsPayload))
            .build()
            .expect("tool spec is valid")
    }
}

enum UpdateOutcome {
    Modified(PathBuf),
    Moved(PathBuf, PathBuf),
    AlreadyPresent(PathBuf),
}

#[derive(Debug)]
enum OperationOutcome {
    Applied(String),
    AlreadyPresent(String),
    NotApplied { operation: String, reason: String },
}

async fn apply_hunk(executor: &dyn swiftide_core::ToolExecutor, hunk: Hunk) -> OperationOutcome {
    let format_err =
        |path: &Path, err: ToolError| format!("{path}: {err}", path = path.display(), err = err);

    match hunk {
        Hunk::Add { path, contents } => {
            if let Err(err) = validate_patch_path(&path) {
                return OperationOutcome::NotApplied {
                    operation: format!("Add: {}", path.display()),
                    reason: format_err(&path, err),
                };
            }
            match apply_add_file(executor, &path, contents).await {
                Ok(()) => OperationOutcome::Applied(format!("Added: {}", path.display())),
                Err(err) => OperationOutcome::NotApplied {
                    operation: format!("Add: {}", path.display()),
                    reason: format_err(&path, err),
                },
            }
        }
        Hunk::Delete { path } => {
            if let Err(err) = validate_patch_path(&path) {
                return OperationOutcome::NotApplied {
                    operation: format!("Delete: {}", path.display()),
                    reason: format_err(&path, err),
                };
            }
            match apply_delete_file(executor, &path).await {
                Ok(()) => OperationOutcome::Applied(format!("Deleted: {}", path.display())),
                Err(err) => OperationOutcome::NotApplied {
                    operation: format!("Delete: {}", path.display()),
                    reason: format_err(&path, err),
                },
            }
        }
        Hunk::Update {
            path,
            move_path,
            chunks,
        } => {
            if let Err(err) = validate_patch_path(&path) {
                return OperationOutcome::NotApplied {
                    operation: format!("Update: {}", path.display()),
                    reason: format_err(&path, err),
                };
            }
            if let Some(dest) = &move_path
                && let Err(err) = validate_patch_path(dest)
            {
                return OperationOutcome::NotApplied {
                    operation: format!("Move: {} -> {}", path.display(), dest.display()),
                    reason: format_err(dest, err),
                };
            }

            match apply_update_file(executor, &path, move_path.as_ref(), &chunks).await {
                Ok(UpdateOutcome::Modified(target)) => {
                    OperationOutcome::Applied(format!("Modified: {}", target.display()))
                }
                Ok(UpdateOutcome::Moved(from, to)) => OperationOutcome::Applied(format!(
                    "Moved: {} -> {}",
                    from.display(),
                    to.display()
                )),
                Ok(UpdateOutcome::AlreadyPresent(target)) => {
                    OperationOutcome::AlreadyPresent(format!("Modified: {}", target.display()))
                }
                Err(err) => OperationOutcome::NotApplied {
                    operation: format!("Update: {}", path.display()),
                    reason: format_err(&path, err),
                },
            }
        }
    }
}

enum Hunk {
    Add {
        path: PathBuf,
        contents: String,
    },
    Delete {
        path: PathBuf,
    },
    Update {
        path: PathBuf,
        move_path: Option<PathBuf>,
        chunks: Vec<UpdateFileChunk>,
    },
}

struct ParsedPatch {
    hunks: Vec<Hunk>,
}

#[derive(Clone)]
struct UpdateFileChunk {
    change_context: Option<String>,
    old_lines: Vec<String>,
    new_lines: Vec<String>,
    is_end_of_file: bool,
}

impl UpdateFileChunk {
    fn is_navigation(&self) -> bool {
        self.old_lines == self.new_lines
            && (!self.old_lines.is_empty() || self.change_context.is_some())
    }
}

async fn apply_add_file(
    executor: &dyn swiftide_core::ToolExecutor,
    path: &Path,
    contents: String,
) -> Result<(), ToolError> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        let cmd = Command::shell(format!("mkdir -p {}", escape(parent.to_string_lossy())));
        executor
            .exec_cmd(&cmd)
            .await
            .map_err(|err| ToolError::unknown(anyhow!(err)))?;
    }

    let command = Command::write_file(path.to_path_buf(), contents);
    executor
        .exec_cmd(&command)
        .await
        .map_err(|err| ToolError::unknown(anyhow!(err)))?;
    Ok(())
}

async fn apply_delete_file(
    executor: &dyn swiftide_core::ToolExecutor,
    path: &Path,
) -> Result<(), ToolError> {
    let read_cmd = Command::read_file(path.to_path_buf());
    executor.exec_cmd(&read_cmd).await.map_err(|err| {
        ToolError::unknown(anyhow!(format!(
            "failed to read {path}: {err}",
            path = path.display()
        )))
    })?;

    let shell_cmd = Command::shell(format!("rm {}", escape(path.to_string_lossy())));
    executor.exec_cmd(&shell_cmd).await.map_err(|err| {
        ToolError::unknown(anyhow!(format!(
            "failed to remove {path}: {err}",
            path = path.display()
        )))
    })?;
    Ok(())
}

async fn apply_update_file(
    executor: &dyn swiftide_core::ToolExecutor,
    source: &Path,
    move_path: Option<&PathBuf>,
    chunks: &[UpdateFileChunk],
) -> Result<UpdateOutcome, ToolError> {
    let read_cmd = Command::read_file(source.to_path_buf());
    let original = executor
        .exec_cmd(&read_cmd)
        .await
        .map_err(|err| {
            ToolError::unknown(anyhow!(format!(
                "failed to read {source}: {err}",
                source = source.display()
            )))
        })?
        .into_bytes();
    let original = String::from_utf8(original).map_err(anyhow::Error::from)?;

    let update = apply_chunks_to_contents(source, &original, chunks)?;

    if let Some(dest) = move_path {
        let same_file = executor
            .exec_cmd(&Command::shell(format!(
                "if [ {} -ef {} ]; then printf same; fi",
                escape(source.to_string_lossy()),
                escape(dest.to_string_lossy()),
            )))
            .await?;
        if !same_file.is_empty() {
            return Err(ToolError::unknown(anyhow!(
                "source and destination refer to the same file; no changes were written"
            )));
        }
        if let Some(parent) = dest.parent().filter(|p| !p.as_os_str().is_empty()) {
            let mkdir_cmd =
                Command::shell(format!("mkdir -p {}", escape(parent.to_string_lossy())));
            executor
                .exec_cmd(&mkdir_cmd)
                .await
                .map_err(|err| ToolError::unknown(anyhow!(err)))?;
        }

        let write_cmd = Command::write_file(dest.clone(), update.contents);
        executor.exec_cmd(&write_cmd).await.map_err(|err| {
            ToolError::unknown(anyhow!(format!(
                "failed to write moved file {dest}: {err}",
                dest = dest.display()
            )))
        })?;

        let remove_cmd = Command::shell(format!("rm {}", escape(source.to_string_lossy())));
        executor.exec_cmd(&remove_cmd).await.map_err(|err| {
            ToolError::unknown(anyhow!(format!(
                "destination {dest} was written, but source {source} was not removed: {err}",
                dest = dest.display(),
                source = source.display()
            )))
        })?;

        Ok(UpdateOutcome::Moved(source.to_path_buf(), dest.clone()))
    } else if !update.changed {
        Ok(UpdateOutcome::AlreadyPresent(source.to_path_buf()))
    } else {
        let write_cmd = Command::write_file(source.to_path_buf(), update.contents);
        executor.exec_cmd(&write_cmd).await.map_err(|err| {
            ToolError::unknown(anyhow!(format!(
                "failed to write {source}: {err}",
                source = source.display()
            )))
        })?;

        Ok(UpdateOutcome::Modified(source.to_path_buf()))
    }
}

struct DerivedUpdate {
    contents: String,
    changed: bool,
}

fn apply_chunks_to_contents(
    path: &Path,
    original: &str,
    chunks: &[UpdateFileChunk],
) -> Result<DerivedUpdate, ToolError> {
    derive_new_contents_from_chunks(original, path, chunks)
        .map_err(|err| ToolError::unknown(anyhow!(err)))
}

fn derive_new_contents_from_chunks(
    original_contents: &str,
    path: &Path,
    chunks: &[UpdateFileChunk],
) -> anyhow::Result<DerivedUpdate> {
    let line_ending = if original_contents.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let normalized = original_contents.replace("\r\n", "\n");
    let had_final_newline = normalized.ends_with('\n');
    let mut lines: Vec<String> = normalized.split('\n').map(str::to_string).collect();

    if had_final_newline {
        lines.pop();
    }

    let changed = apply_chunks_to_lines(&mut lines, path, chunks)?;
    if had_final_newline {
        lines.push(String::new());
    }

    Ok(DerivedUpdate {
        contents: lines.join(line_ending),
        changed,
    })
}

fn apply_chunks_to_lines(
    lines: &mut Vec<String>,
    path: &Path,
    chunks: &[UpdateFileChunk],
) -> anyhow::Result<bool> {
    let mut cursor = PatchCursor::default();
    let mut changed = false;

    for (chunk_index, chunk) in chunks.iter().enumerate() {
        resolve_context_anchor(lines, path, chunk, chunk_index, &mut cursor)?;
        if apply_navigation(lines, chunk, chunk_index, &mut cursor)? {
            continue;
        }
        changed |= apply_edit(lines, path, chunk, chunk_index, &mut cursor)?;
    }

    Ok(changed)
}

#[derive(Default)]
struct PatchCursor {
    line: usize,
    anchor: Option<MatchAnchor>,
}

impl PatchCursor {
    fn is_anchored(&self) -> bool {
        self.anchor.is_some()
    }

    fn description(&self) -> String {
        anchor_description(
            self.anchor
                .as_ref()
                .map(|anchor| anchor.description.as_str()),
        )
    }

    fn ambiguity_end(&self) -> Option<usize> {
        self.anchor.as_ref().and_then(|anchor| anchor.end)
    }

    fn clear_anchor(&mut self) {
        self.anchor = None;
    }
}

struct MatchAnchor {
    description: String,
    end: Option<usize>,
}

fn resolve_context_anchor(
    lines: &[String],
    path: &Path,
    chunk: &UpdateFileChunk,
    chunk_index: usize,
    cursor: &mut PatchCursor,
) -> anyhow::Result<()> {
    let Some(context_line) = &chunk.change_context else {
        return Ok(());
    };

    let found = match seek_sequence(lines, std::slice::from_ref(context_line), cursor, false) {
        SequenceMatch::NotFound => seek_anchor_prefix(lines, context_line, cursor),
        found => found,
    };

    match found {
        SequenceMatch::Found { line } => {
            set_anchor(lines, line, 1, context_line.clone(), cursor);
            Ok(())
        }
        SequenceMatch::Ambiguous { candidates, method } => bail!(ambiguous_message(
            chunk_index,
            cursor
                .anchor
                .as_ref()
                .map(|anchor| anchor.description.as_str()),
            method,
            &candidates,
            std::slice::from_ref(context_line),
        )),
        SequenceMatch::NotFound => bail!(
            "hunk {}: failed to find anchor '{context_line}' in {}",
            chunk_index + 1,
            path.display()
        ),
    }
}

fn apply_navigation(
    lines: &[String],
    chunk: &UpdateFileChunk,
    chunk_index: usize,
    cursor: &mut PatchCursor,
) -> anyhow::Result<bool> {
    if !chunk.is_navigation() {
        return Ok(false);
    }
    if chunk.old_lines.is_empty() {
        cursor.anchor = Some(MatchAnchor {
            description: chunk
                .change_context
                .clone()
                .unwrap_or_else(|| "the current position".to_string()),
            end: cursor.ambiguity_end(),
        });
        return Ok(true);
    }

    match seek_sequence(lines, &chunk.old_lines, cursor, chunk.is_end_of_file) {
        SequenceMatch::Found { line } => {
            set_anchor(
                lines,
                line,
                chunk.old_lines.len(),
                chunk
                    .old_lines
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "the current position".to_string()),
                cursor,
            );
            Ok(true)
        }
        SequenceMatch::Ambiguous { candidates, method } => bail!(ambiguous_message(
            chunk_index,
            cursor
                .anchor
                .as_ref()
                .map(|anchor| anchor.description.as_str()),
            method,
            &candidates,
            &chunk.old_lines,
        )),
        SequenceMatch::NotFound => bail!(
            "hunk {}: failed to find navigation context after {}:\n{}",
            chunk_index + 1,
            cursor.description(),
            chunk.old_lines.join("\n")
        ),
    }
}

fn apply_edit(
    lines: &mut Vec<String>,
    path: &Path,
    chunk: &UpdateFileChunk,
    chunk_index: usize,
    cursor: &mut PatchCursor,
) -> anyhow::Result<bool> {
    if chunk.old_lines.is_empty() {
        let insertion_index = if cursor.is_anchored() {
            cursor.line
        } else {
            lines.len()
        };
        let existing_index = if cursor.is_anchored() {
            Some(insertion_index)
        } else {
            lines.len().checked_sub(chunk.new_lines.len())
        };
        if let Some(start) = existing_index
            && lines.get(start..start + chunk.new_lines.len()) == Some(chunk.new_lines.as_slice())
        {
            cursor.line = start + chunk.new_lines.len();
            cursor.clear_anchor();
            return Ok(false);
        }
        lines.splice(
            insertion_index..insertion_index,
            chunk.new_lines.iter().cloned(),
        );
        cursor.line = insertion_index + chunk.new_lines.len();
        cursor.clear_anchor();
        return Ok(true);
    }

    let (old_lines, new_lines, found) =
        find_chunk(lines, chunk, cursor, &chunk.old_lines, &chunk.new_lines);
    match found {
        SequenceMatch::Found { line } => {
            lines.splice(line..line + old_lines.len(), new_lines.iter().cloned());
            cursor.line = line + new_lines.len();
            cursor.clear_anchor();
            Ok(old_lines != new_lines)
        }
        SequenceMatch::Ambiguous { candidates, method } => bail!(ambiguous_message(
            chunk_index,
            cursor
                .anchor
                .as_ref()
                .map(|anchor| anchor.description.as_str()),
            method,
            &candidates,
            old_lines,
        )),
        SequenceMatch::NotFound => recognize_already_present(
            lines,
            path,
            chunk,
            chunk_index,
            cursor,
            old_lines,
            new_lines,
        ),
    }
}

fn recognize_already_present(
    lines: &[String],
    path: &Path,
    chunk: &UpdateFileChunk,
    chunk_index: usize,
    cursor: &mut PatchCursor,
    old_lines: &[String],
    new_lines: &[String],
) -> anyhow::Result<bool> {
    match seek_sequence(lines, new_lines, cursor, chunk.is_end_of_file) {
        SequenceMatch::Found { line } => {
            cursor.line = line + new_lines.len();
            cursor.clear_anchor();
            Ok(false)
        }
        SequenceMatch::Ambiguous { candidates, method } => bail!(ambiguous_message(
            chunk_index,
            cursor
                .anchor
                .as_ref()
                .map(|anchor| anchor.description.as_str()),
            method,
            &candidates,
            new_lines,
        )),
        SequenceMatch::NotFound => bail!(
            "hunk {}: expected lines not found after {} in {}:\n{}",
            chunk_index + 1,
            cursor.description(),
            path.display(),
            old_lines.join("\n")
        ),
    }
}

fn find_chunk<'a>(
    lines: &[String],
    chunk: &UpdateFileChunk,
    cursor: &PatchCursor,
    old_lines: &'a [String],
    new_lines: &'a [String],
) -> (&'a [String], &'a [String], SequenceMatch) {
    let found = seek_sequence(lines, old_lines, cursor, chunk.is_end_of_file);
    if !matches!(found, SequenceMatch::NotFound)
        || !old_lines.last().is_some_and(String::is_empty)
        || !new_lines.last().is_some_and(String::is_empty)
    {
        return (old_lines, new_lines, found);
    }

    let old_lines = &old_lines[..old_lines.len() - 1];
    let new_lines = &new_lines[..new_lines.len() - 1];
    let found = seek_sequence(lines, old_lines, cursor, chunk.is_end_of_file);
    (old_lines, new_lines, found)
}

#[derive(Clone, Copy, Debug)]
enum MatchMethod {
    Exact,
    TrimEnd,
    Trim,
    Normalized,
    AnchorPrefix,
}

enum SequenceMatch {
    NotFound,
    Found {
        line: usize,
    },
    Ambiguous {
        candidates: Vec<usize>,
        method: MatchMethod,
    },
}

fn seek_sequence(
    lines: &[String],
    pattern: &[String],
    cursor: &PatchCursor,
    eof: bool,
) -> SequenceMatch {
    if pattern.is_empty() {
        return SequenceMatch::Found { line: cursor.line };
    }

    if pattern.len() > lines.len() || cursor.line > lines.len().saturating_sub(pattern.len()) {
        return SequenceMatch::NotFound;
    }

    let search_start = if eof && lines.len() >= pattern.len() {
        lines.len() - pattern.len()
    } else {
        cursor.line
    };

    let search_end = lines.len().saturating_sub(pattern.len());

    if let Some(ambiguity_end) = cursor.ambiguity_end() {
        let scoped_end = ambiguity_end.saturating_sub(1).min(search_end);
        if search_start <= scoped_end {
            let found = seek_sequence_in_range(lines, pattern, search_start, scoped_end);
            if !matches!(found, SequenceMatch::NotFound) {
                return found;
            }
        }
    }

    seek_sequence_in_range(lines, pattern, search_start, search_end)
}

fn seek_sequence_in_range(
    lines: &[String],
    pattern: &[String],
    search_start: usize,
    search_end: usize,
) -> SequenceMatch {
    if let Some(found) = select_match(search_start, search_end, MatchMethod::Exact, |line| {
        lines[line..line + pattern.len()] == *pattern
    }) {
        return found;
    }
    if let Some(found) = select_match(search_start, search_end, MatchMethod::TrimEnd, |line| {
        matches_trim_end(lines, pattern, line)
    }) {
        return found;
    }
    if let Some(found) = select_match(search_start, search_end, MatchMethod::Trim, |line| {
        matches_trimmed(lines, pattern, line)
    }) {
        return found;
    }
    if let Some(found) = select_match(search_start, search_end, MatchMethod::Normalized, |line| {
        matches_normalized(lines, pattern, line)
    }) {
        return found;
    }

    SequenceMatch::NotFound
}

fn seek_anchor_prefix(lines: &[String], anchor: &str, cursor: &PatchCursor) -> SequenceMatch {
    if lines.is_empty() || cursor.line >= lines.len() {
        return SequenceMatch::NotFound;
    }

    let search_start = cursor.line;
    let search_end = lines.len() - 1;
    if let Some(ambiguity_end) = cursor.ambiguity_end() {
        let scoped_end = ambiguity_end.saturating_sub(1).min(search_end);
        if search_start <= scoped_end {
            let found = seek_anchor_prefix_in_range(lines, anchor, search_start, scoped_end);
            if !matches!(found, SequenceMatch::NotFound) {
                return found;
            }
        }
    }

    seek_anchor_prefix_in_range(lines, anchor, search_start, search_end)
}

fn seek_anchor_prefix_in_range(
    lines: &[String],
    anchor: &str,
    search_start: usize,
    search_end: usize,
) -> SequenceMatch {
    select_match(
        search_start,
        search_end,
        MatchMethod::AnchorPrefix,
        |line| matches_anchor_prefix(&lines[line], anchor),
    )
    .unwrap_or(SequenceMatch::NotFound)
}

fn matches_anchor_prefix(line: &str, anchor: &str) -> bool {
    let line = normalize(line);
    let anchor = normalize(anchor);
    line.strip_prefix(&anchor).is_some_and(|suffix| {
        suffix.chars().next().is_some_and(|character| {
            character.is_whitespace() || matches!(character, '{' | '(' | '[' | ':' | '<')
        })
    })
}

fn set_anchor(
    lines: &[String],
    line: usize,
    pattern_len: usize,
    description: String,
    cursor: &mut PatchCursor,
) {
    let existing_end = cursor.ambiguity_end();
    cursor.line = line + pattern_len;
    let sibling_end = sibling_boundary(
        lines,
        line,
        pattern_len,
        existing_end.unwrap_or(lines.len()),
    );
    cursor.anchor = Some(MatchAnchor {
        description,
        end: match (existing_end, sibling_end) {
            (Some(existing), Some(sibling)) => Some(existing.min(sibling)),
            (Some(existing), None) => Some(existing),
            (None, sibling) => sibling,
        },
    });
}

fn sibling_boundary(
    lines: &[String],
    anchor_line: usize,
    pattern_len: usize,
    search_end: usize,
) -> Option<usize> {
    let anchor = lines.get(anchor_line)?;
    let anchor_content = anchor.trim_start();
    let anchor_indent = &anchor[..anchor.len() - anchor_content.len()];
    let anchor_token = anchor_content.split_whitespace().next()?;

    lines
        .iter()
        .enumerate()
        .take(search_end)
        .skip(anchor_line + pattern_len)
        .find_map(|(line, candidate)| {
            let candidate_content = candidate.trim_start();
            let candidate_indent = &candidate[..candidate.len() - candidate_content.len()];
            (candidate_indent == anchor_indent
                && candidate_content.split_whitespace().next() == Some(anchor_token))
            .then_some(line)
        })
}

fn matches_trim_end(lines: &[String], pattern: &[String], line: usize) -> bool {
    pattern
        .iter()
        .enumerate()
        .all(|(offset, expected)| lines[line + offset].trim_end() == expected.trim_end())
}

fn matches_trimmed(lines: &[String], pattern: &[String], line: usize) -> bool {
    pattern
        .iter()
        .enumerate()
        .all(|(offset, expected)| lines[line + offset].trim() == expected.trim())
}

fn matches_normalized(lines: &[String], pattern: &[String], line: usize) -> bool {
    pattern
        .iter()
        .enumerate()
        .all(|(offset, expected)| normalize(&lines[line + offset]) == normalize(expected))
}

fn normalize(input: &str) -> String {
    input
        .trim()
        .chars()
        .map(|character| match character {
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}'
            | '\u{2212}' => '-',
            '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' => '\'',
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' => '"',
            '\u{00A0}' | '\u{2002}' | '\u{2003}' | '\u{2004}' | '\u{2005}' | '\u{2006}'
            | '\u{2007}' | '\u{2008}' | '\u{2009}' | '\u{200A}' | '\u{202F}' | '\u{205F}'
            | '\u{3000}' => ' ',
            other => other,
        })
        .collect()
}

fn find_matches(
    search_start: usize,
    search_end: usize,
    mut matches_at: impl FnMut(usize) -> bool,
) -> Vec<usize> {
    let mut candidates = Vec::with_capacity(5);
    for line in search_start..=search_end {
        if matches_at(line) {
            candidates.push(line);
            if candidates.len() == 5 {
                break;
            }
        }
    }
    candidates
}

fn select_match(
    search_start: usize,
    search_end: usize,
    method: MatchMethod,
    matches_at: impl FnMut(usize) -> bool,
) -> Option<SequenceMatch> {
    let candidates = find_matches(search_start, search_end, matches_at);
    match candidates.as_slice() {
        [] => None,
        [line] => Some(SequenceMatch::Found { line: *line }),
        _ => Some(SequenceMatch::Ambiguous { candidates, method }),
    }
}

fn ambiguous_message(
    chunk_index: usize,
    anchor: Option<&str>,
    method: MatchMethod,
    candidates: &[usize],
    pattern: &[String],
) -> String {
    let candidate_lines = candidates
        .iter()
        .take(5)
        .map(|line| (line + 1).to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "hunk {}: ambiguous {method:?} match after {}; candidate lines: {candidate_lines}\n{}",
        chunk_index + 1,
        anchor_description(anchor),
        pattern.join("\n")
    )
}

fn anchor_description(anchor: Option<&str>) -> String {
    anchor.map_or_else(
        || "the start of the file".to_string(),
        |anchor| format!("anchor '{anchor}'"),
    )
}

#[allow(clippy::too_many_lines)]
fn parse_patch(input: &str) -> anyhow::Result<ParsedPatch> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("patch cannot be empty");
    }

    let lines: Vec<&str> = trimmed.lines().collect();
    if !matches!(
        lines.first().map(|line| line.trim()),
        Some("*** Begin Patch")
    ) {
        bail!("patch must start with '*** Begin Patch'");
    }
    if !matches!(lines.last().map(|line| line.trim()), Some("*** End Patch")) {
        bail!("patch must end with '*** End Patch'");
    }

    let mut idx = 1usize;
    let end = lines.len().saturating_sub(1);
    let mut hunks = Vec::new();

    while idx < end {
        let current = lines[idx].trim();
        if current.is_empty() {
            idx += 1;
            continue;
        }

        if let Some(rest) = current.strip_prefix("*** Add File: ") {
            let path = PathBuf::from(rest.trim());
            idx += 1;
            let mut content_lines = Vec::new();
            while idx < end {
                let line = lines[idx];
                let trimmed_line = line.trim();
                if trimmed_line.starts_with("***") {
                    break;
                }
                if trimmed_line.starts_with("@@") {
                    bail!(
                        "line {}: unexpected hunk header while parsing add file for {}",
                        idx + 1,
                        path.display()
                    );
                }
                if !line.starts_with('+') {
                    bail!(
                        "line {}: lines inside an Add File section must start with '+', found '{line}'",
                        idx + 1
                    );
                }
                content_lines.push(line[1..].to_string());
                idx += 1;
            }
            let mut contents = content_lines.join("\n");
            contents.push('\n');
            hunks.push(Hunk::Add { path, contents });
            continue;
        }

        if let Some(rest) = current.strip_prefix("*** Delete File: ") {
            let path = PathBuf::from(rest.trim());
            idx += 1;
            hunks.push(Hunk::Delete { path });
            continue;
        }

        if let Some(rest) = current.strip_prefix("*** Update File: ") {
            let path = PathBuf::from(rest.trim());
            idx += 1;
            let mut move_path = None;
            if idx < end {
                let maybe_move = lines[idx].trim();
                if let Some(dest) = maybe_move.strip_prefix("*** Move to: ") {
                    move_path = Some(PathBuf::from(dest.trim()));
                    idx += 1;
                }
            }

            let mut chunks = Vec::new();
            while idx < end {
                let raw = lines[idx];
                let line = raw.trim();
                if line.starts_with("***") && line != "*** End of File" {
                    break;
                }
                if is_blank_hunk_separator(&lines, idx) {
                    idx += 1;
                    continue;
                }
                let starts_implicit_first_hunk = chunks.is_empty()
                    && (raw.is_empty()
                        || raw.starts_with(' ')
                        || raw.starts_with('-')
                        || raw.starts_with('+'));
                if !line.starts_with("@@") && !starts_implicit_first_hunk {
                    bail!(
                        "line {}: expected '@@' hunk header, found '{}'",
                        idx + 1,
                        lines[idx]
                    );
                }
                let (chunk, next_idx) = parse_update_chunk(&lines, idx, end)?;
                chunks.push(chunk);
                idx = next_idx;
            }

            if chunks.is_empty() {
                bail!(
                    "update for '{}' must contain at least one hunk",
                    path.display()
                );
            }

            hunks.push(Hunk::Update {
                path,
                move_path,
                chunks,
            });
            continue;
        }

        bail!("line {}: unknown patch directive '{current}'", idx + 1);
    }

    Ok(ParsedPatch { hunks })
}

fn parse_update_chunk(
    lines: &[&str],
    start: usize,
    end: usize,
) -> anyhow::Result<(UpdateFileChunk, usize)> {
    let header = lines[start].trim();
    let (change_context, mut idx) = match header {
        "@@" => (None, start + 1),
        _ if header.starts_with("@@ ") => (Some(header[3..].trim().to_string()), start + 1),
        _ if lines[start].is_empty()
            || lines[start].starts_with(' ')
            || lines[start].starts_with('-')
            || lines[start].starts_with('+') =>
        {
            (None, start)
        }
        _ => bail!("line {}: invalid hunk header '{header}'", start + 1),
    };

    let mut old_lines = Vec::new();
    let mut new_lines = Vec::new();
    let mut is_end_of_file = false;

    while idx < end {
        let raw = lines[idx];
        if is_blank_hunk_separator(lines, idx) {
            idx += 1;
            break;
        }
        let prefix = raw.chars().next();
        match prefix {
            Some(' ') => {
                let remainder = raw[1..].to_string();
                old_lines.push(remainder.clone());
                new_lines.push(remainder);
            }
            Some('-') => old_lines.push(raw[1..].to_string()),
            Some('+') => new_lines.push(raw[1..].to_string()),
            _ if raw.is_empty() => {
                old_lines.push(String::new());
                new_lines.push(String::new());
            }
            _ if raw.starts_with("@@") => break,
            _ if raw.starts_with("***") => {
                if raw == "*** End of File" {
                    is_end_of_file = true;
                    idx += 1;
                }
                break;
            }
            Some(prefix) => bail!("line {}: unexpected hunk line prefix '{prefix}'", idx + 1),
            None => {}
        }
        idx += 1;
    }

    if old_lines.is_empty() && new_lines.is_empty() && change_context.is_none() {
        bail!(
            "line {}: hunk must contain a change or a navigation anchor",
            start + 1
        );
    }

    Ok((
        UpdateFileChunk {
            change_context,
            old_lines,
            new_lines,
            is_end_of_file,
        },
        idx,
    ))
}

fn is_blank_hunk_separator(lines: &[&str], index: usize) -> bool {
    lines[index].is_empty()
        && lines.get(index + 1).is_some_and(|next| {
            let next = next.trim();
            next.starts_with("@@") || (next.starts_with("***") && next != "*** End of File")
        })
}

fn validate_patch_path(path: &Path) -> Result<(), ToolError> {
    if path.is_absolute() {
        return Err(ToolError::unknown(anyhow!(
            "absolute paths are not supported"
        )));
    }

    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(ToolError::unknown(anyhow!(
            "paths cannot contain parent directory components"
        )));
    }

    if path.components().next().is_none() {
        return Err(ToolError::unknown(anyhow!("empty paths are not supported")));
    }

    Ok(())
}

impl ApplyPatchSummary {
    fn render(self) -> String {
        let total = self.operations.len();
        let reached = self
            .operations
            .iter()
            .filter(|operation| {
                matches!(
                    operation,
                    OperationOutcome::Applied(_) | OperationOutcome::AlreadyPresent(_)
                )
            })
            .count();
        let heading = if reached == total {
            format!(
                "Patch applied successfully ({reached}/{total} operations reached requested state)."
            )
        } else if reached == 0 {
            format!("Patch not applied (0/{total} operations reached requested state).")
        } else {
            format!(
                "Patch partially applied ({reached}/{total} operations reached requested state)."
            )
        };

        let mut sections = Vec::new();
        append_operation_section(
            &mut sections,
            "Applied",
            self.operations
                .iter()
                .filter_map(|operation| match operation {
                    OperationOutcome::Applied(description) => Some(description.as_str()),
                    _ => None,
                }),
        );
        append_operation_section(
            &mut sections,
            "Already present",
            self.operations
                .iter()
                .filter_map(|operation| match operation {
                    OperationOutcome::AlreadyPresent(description) => Some(description.as_str()),
                    _ => None,
                }),
        );

        let not_applied = self
            .operations
            .iter()
            .filter_map(|operation| match operation {
                OperationOutcome::NotApplied { operation, reason } => {
                    Some(format!("{operation}: {reason}"))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if !not_applied.is_empty() {
            sections.push(format!(
                "Not applied:\n{}",
                not_applied
                    .iter()
                    .map(|operation| format!("- {operation}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }

        if sections.is_empty() {
            heading
        } else {
            format!("{heading}\n{}", sections.join("\n"))
        }
    }
}

fn append_operation_section<'a>(
    sections: &mut Vec<String>,
    heading: &str,
    operations: impl Iterator<Item = &'a str>,
) {
    let operations = operations.collect::<Vec<_>>();
    if !operations.is_empty() {
        sections.push(format!(
            "{heading}:\n{}",
            operations
                .iter()
                .map(|operation| format!("- {operation}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[tokio::test]
    async fn moving_to_the_same_file_preserves_source_bytes() {
        use swiftide_agents::tools::local_executor::LocalExecutor;
        for destination in ["file.txt", "./file.txt", "symlink.txt", "hardlink.txt"] {
            let dir = temp_dir::TempDir::new().unwrap();
            let source = dir.path().join("file.txt");
            std::fs::write(&source, b"old\n").unwrap();
            std::os::unix::fs::symlink("file.txt", dir.path().join("symlink.txt")).unwrap();
            std::fs::hard_link(&source, dir.path().join("hardlink.txt")).unwrap();
            let executor = LocalExecutor::builder()
                .workdir(dir.path())
                .build()
                .unwrap();
            let parsed = parse_patch(&format!(
                "*** Begin Patch
*** Update File: file.txt
*** Move to: {destination}
@@
-old
+new
*** End Patch"
            ))
            .unwrap();
            let Hunk::Update { chunks, .. } = &parsed.hunks[0] else {
                panic!("update expected")
            };
            assert!(
                apply_update_file(
                    &executor,
                    Path::new("file.txt"),
                    Some(&PathBuf::from(destination)),
                    chunks
                )
                .await
                .is_err(),
                "{destination}"
            );
            assert_eq!(std::fs::read(&source).unwrap(), b"old\n", "{destination}");
        }
    }

    #[tokio::test]
    async fn patch_refuses_invalid_utf8_without_changing_bytes() {
        use swiftide_agents::tools::local_executor::LocalExecutor;
        let dir = temp_dir::TempDir::new().unwrap();
        let source = dir.path().join("file.txt");
        std::fs::write(&source, b"old\n\xff\n").unwrap();
        let executor = LocalExecutor::builder()
            .workdir(dir.path())
            .build()
            .unwrap();
        let parsed = parse_patch(
            "*** Begin Patch\n*** Update File: file.txt\n@@\n-old\n+new\n*** End Patch",
        )
        .unwrap();
        let Hunk::Update { chunks, .. } = &parsed.hunks[0] else {
            panic!("update expected")
        };
        assert!(
            apply_update_file(&executor, Path::new("file.txt"), None, chunks)
                .await
                .is_err()
        );
        assert_eq!(std::fs::read(&source).unwrap(), b"old\n\xff\n");
    }
}
