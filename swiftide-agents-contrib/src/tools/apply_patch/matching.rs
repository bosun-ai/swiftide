use super::parser::UpdateFileChunk;
use anyhow::{anyhow, bail};
use std::path::Path;
use swiftide_core::chat_completion::errors::ToolError;

pub(super) struct DerivedUpdate {
    pub(super) contents: String,
    pub(super) changed: bool,
}

pub(super) fn apply_chunks_to_contents(
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

    let (replacement, found) = find_chunk(lines, chunk, cursor);
    let Replacement {
        old_lines,
        new_lines,
        ..
    } = replacement;
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
        SequenceMatch::NotFound => {
            recognize_already_present(lines, path, chunk_index, cursor, replacement)
        }
    }
}

#[derive(Clone, Copy)]
struct Replacement<'a> {
    old_lines: &'a [String],
    new_lines: &'a [String],
    end_of_file: bool,
}

fn recognize_already_present(
    lines: &[String],
    path: &Path,
    chunk_index: usize,
    cursor: &mut PatchCursor,
    replacement: Replacement<'_>,
) -> anyhow::Result<bool> {
    let Replacement {
        old_lines,
        new_lines,
        end_of_file,
    } = replacement;
    match seek_sequence(lines, new_lines, cursor, end_of_file) {
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
    chunk: &'a UpdateFileChunk,
    cursor: &PatchCursor,
) -> (Replacement<'a>, SequenceMatch) {
    let mut replacement = Replacement {
        old_lines: &chunk.old_lines,
        new_lines: &chunk.new_lines,
        end_of_file: chunk.is_end_of_file,
    };
    let found = seek_sequence(lines, replacement.old_lines, cursor, chunk.is_end_of_file);
    if !matches!(found, SequenceMatch::NotFound)
        || !replacement.old_lines.last().is_some_and(String::is_empty)
        || !replacement.new_lines.last().is_some_and(String::is_empty)
    {
        return (replacement, found);
    }
    replacement.old_lines = &replacement.old_lines[..replacement.old_lines.len() - 1];
    replacement.new_lines = &replacement.new_lines[..replacement.new_lines.len() - 1];
    let found = seek_sequence(lines, replacement.old_lines, cursor, chunk.is_end_of_file);
    (replacement, found)
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
