use std::{borrow::Cow, fmt::Write as _};

use crate::command::accept_non_zero_exit;
use async_trait::async_trait;
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use swiftide_core::{
    AgentContext, Command,
    chat_completion::{Tool, ToolCall, ToolOutput, ToolSpec, errors::ToolError},
};

/// Read file pages through the executor (tool name: `read_file`).
///
/// Accepts `file_name`, optional one-based `offset` (zero starts at one), and optional `limit`.
/// Defaults to 2,000 lines, cuts long lines at 2,000 characters, and refuses windows over
/// 100,000 characters. Partial pages explain how to continue.
/// Relative and absolute paths are passed to the executor unchanged.
///
/// # Examples
///
/// ```no_run
/// use swiftide_agents::Agent;
/// use swiftide_agents_contrib::tools::ReadFile;
/// Agent::builder().add_tool(ReadFile::default());
/// ```
#[derive(Clone, Debug, Default)]
pub struct ReadFile {
    numbered: bool,
}

impl ReadFile {
    /// Register `read_file_with_line_numbers`, retaining original file line numbers.
    ///
    /// Both modes use the same paging and output limits.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents::Agent;
    /// use swiftide_agents_contrib::tools::ReadFile;
    /// Agent::builder().add_tool(ReadFile::default().with_line_numbers());
    /// ```
    #[must_use]
    pub fn with_line_numbers(mut self) -> Self {
        self.numbered = true;
        self
    }
}

/// Lines a file read returns when the caller gives no `limit`.
const DEFAULT_READ_LINES: usize = 2_000;
/// Characters kept from a single line; the rest of a longer line is cut.
const MAX_READ_LINE_CHARS: usize = 2_000;
/// Largest window a file read returns: ~25,000 tokens at the summarizer's four characters per
/// token. File reads are exempt from tool summarization, so this bound keeps them out of context.
const MAX_READ_CHARS: usize = 100_000;
const TRUNCATED_LINE_MARKER: &str = "… [line truncated]";

const NUMBERED_READ_DESCRIPTION: &str = "Reads file content, including line numbers. You MUST use this tool to retrieve line numbers before making an edit with edit_file. Reads up to 2000 lines by default. When you already know which part of the file you need, only read that part: use `offset` and `limit`.";
const READ_DESCRIPTION: &str = "Reads file content. Reads up to 2000 lines by default. When you already know which part of the file you need, only read that part: use `offset` and `limit`.";

#[derive(Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
struct ReadFileArgs {
    #[schemars(description = "Full path of the file")]
    file_name: String,
    #[schemars(
        description = "Optional line number to start reading from (1-indexed). Defaults to 1."
    )]
    offset: Option<usize>,
    #[schemars(description = "Optional number of lines to read. Defaults to 2000.")]
    limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
struct ReadFileWithLineNumbersArgs {
    #[schemars(description = "Full path of the file")]
    file_name: String,
    #[schemars(
        description = "Optional line number to start reading from (1-indexed). Defaults to 1. Line numbers stay those of the file."
    )]
    offset: Option<usize>,
    #[schemars(description = "Optional number of lines to read. Defaults to 2000.")]
    limit: Option<usize>,
}

#[async_trait]
impl Tool for ReadFile {
    async fn invoke(
        &self,
        context: &dyn AgentContext,
        call: &ToolCall,
    ) -> Result<ToolOutput, ToolError> {
        let args = call.args().ok_or_else(|| {
            ToolError::missing_arguments(format!("No arguments provided for {}", self.name()))
        })?;
        if self.numbered {
            let args: ReadFileWithLineNumbersArgs = serde_json::from_str(args)?;
            read_file_with_line_numbers(context, &args.file_name, args.offset, args.limit).await
        } else {
            let args: ReadFileArgs = serde_json::from_str(args)?;
            read_file(context, &args.file_name, args.offset, args.limit).await
        }
    }
    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed(if self.numbered {
            "read_file_with_line_numbers"
        } else {
            "read_file"
        })
    }
    fn tool_spec(&self) -> ToolSpec {
        if self.numbered {
            ToolSpec::builder()
                .name("read_file_with_line_numbers")
                .description(NUMBERED_READ_DESCRIPTION)
                .parameters_schema(schema_for!(ReadFileWithLineNumbersArgs))
                .build()
                .expect("tool spec is valid")
        } else {
            ToolSpec::builder()
                .name("read_file")
                .description(READ_DESCRIPTION)
                .parameters_schema(schema_for!(ReadFileArgs))
                .build()
                .expect("tool spec is valid")
        }
    }
}

async fn read_file(
    context: &dyn AgentContext,
    file_name: &str,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<ToolOutput, ToolError> {
    read_file_window(context, file_name, offset, limit, false).await
}

// TODO: Better to have a single read_file tool with an optional line number flag
async fn read_file_with_line_numbers(
    context: &dyn AgentContext,
    file_name: &str,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<ToolOutput, ToolError> {
    read_file_window(context, file_name, offset, limit, true).await
}

async fn read_file_window(
    context: &dyn AgentContext,
    file_name: &str,
    offset: Option<usize>,
    limit: Option<usize>,
    numbered: bool,
) -> Result<ToolOutput, ToolError> {
    // Like Claude Code's Read, offset 0 means the first line.
    let offset = offset.filter(|offset| *offset > 0).unwrap_or(1);
    let limit = limit.unwrap_or(DEFAULT_READ_LINES);
    if limit == 0 {
        return Ok(ToolOutput::fail("limit must be 1 or greater"));
    }
    // i.e. if the file doesn't exist, just forward that message
    let output = accept_non_zero_exit(
        context
            .executor()
            .exec_cmd(&Command::read_file(file_name))
            .await,
    )?;
    let file = output.to_string_lossy();

    Ok(match file_window(&file, offset, limit, numbered) {
        Ok(window) => window.into_owned().into(),
        Err(message) => ToolOutput::fail(message),
    })
}

/// Selects `limit` lines starting at line `offset` (1-indexed) and renders them, numbered with
/// their line in the file when `numbered`. Lines are cut at [`MAX_READ_LINE_CHARS`]; a window over
/// [`MAX_READ_CHARS`] is refused. Content under every limit is returned unchanged.
fn file_window(
    content: &str,
    offset: usize,
    limit: usize,
    numbered: bool,
) -> Result<Cow<'_, str>, String> {
    if content.is_empty() {
        return Ok(Cow::Borrowed("File exists but has empty contents."));
    }

    let mut lines = content.split_inclusive('\n');
    let skipped = lines.by_ref().take(offset - 1).count();
    let mut rendered = String::new();
    let mut chars = 0;
    let mut shown = 0;
    let mut truncated_any = false;
    for (line_number, line) in (offset..).zip(lines.by_ref().take(limit)) {
        shown += 1;
        let text = if numbered {
            strip_line_ending(line)
        } else {
            line
        };
        let (kept, truncated) = match text.char_indices().nth(MAX_READ_LINE_CHARS) {
            Some((cut, _)) => (&text[..cut], true),
            None => (text, false),
        };
        truncated_any |= truncated;
        // Numbered lines also carry their `N|` prefix and a separating newline.
        let numbering = if numbered {
            line_number.ilog10() as usize + 3
        } else {
            0
        };
        chars += kept.chars().count() + numbering;
        if chars > MAX_READ_CHARS {
            // Keep counting so the refusal can say how large the window is.
            continue;
        }
        if numbered {
            if shown > 1 {
                rendered.push('\n');
            }
            let _ = write!(rendered, "{line_number}|");
        }
        rendered.push_str(kept);
        if truncated {
            rendered.push_str(TRUNCATED_LINE_MARKER);
            if !numbered {
                rendered.push_str(line_ending(line));
            }
        }
    }

    if shown == 0 {
        return Ok(Cow::Owned(format!(
            "The file has {skipped} lines; offset {offset} is past the end of the file."
        )));
    }
    if chars > MAX_READ_CHARS {
        return Err(format!(
            "File content (~{tokens} tokens) exceeds the maximum of {max} tokens. Use the offset and limit parameters to read a specific part of the file, or search it first.",
            tokens = chars / 4,
            max = MAX_READ_CHARS / 4,
        ));
    }

    let remaining = lines.count();
    if remaining == 0 {
        return Ok(if !numbered && offset == 1 && !truncated_any {
            Cow::Borrowed(content)
        } else {
            Cow::Owned(rendered)
        });
    }
    if !rendered.ends_with('\n') {
        rendered.push('\n');
    }
    let _ = write!(
        rendered,
        "[File has {remaining} more lines. Use offset={next} to continue.]",
        next = offset + shown
    );
    Ok(Cow::Owned(rendered))
}

/// Mirrors `str::lines`, so numbered output keeps its existing format.
fn strip_line_ending(line: &str) -> &str {
    let line = line.strip_suffix('\n').unwrap_or(line);
    line.strip_suffix('\r').unwrap_or(line)
}

fn line_ending(line: &str) -> &str {
    &line[strip_line_ending(line).len()..]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbered_lines(range: std::ops::RangeInclusive<usize>) -> String {
        range.fold(String::new(), |mut lines, n| {
            let _ = writeln!(lines, "line {n}");
            lines
        })
    }

    #[test]
    fn a_file_under_the_limits_is_returned_unchanged() {
        for content in ["one\ntwo\r\nthree", "one\ntwo\n", "\n\n"] {
            assert!(matches!(
                file_window(content, 1, DEFAULT_READ_LINES, false),
                Ok(Cow::Borrowed(window)) if window == content
            ));
            let old_numbered = content
                .lines()
                .enumerate()
                .map(|(i, l)| format!("{}|{l}", i + 1))
                .collect::<Vec<_>>()
                .join("\n");
            assert_eq!(
                file_window(content, 1, DEFAULT_READ_LINES, true).unwrap(),
                old_numbered
            );
        }
    }

    #[test]
    fn by_default_the_first_2000_lines_are_returned_and_the_rest_announced() {
        let content = numbered_lines(1..=2_500);

        let window = file_window(&content, 1, DEFAULT_READ_LINES, false).unwrap();

        assert_eq!(
            window,
            numbered_lines(1..=2_000) + "[File has 500 more lines. Use offset=2001 to continue.]"
        );
    }

    #[test]
    fn offset_and_limit_return_exactly_that_window_with_file_line_numbers() {
        let content = numbered_lines(1..=10);

        assert_eq!(
            file_window(&content, 4, 2, true).unwrap(),
            "4|line 4\n5|line 5\n[File has 5 more lines. Use offset=6 to continue.]"
        );
        assert_eq!(
            file_window(&content, 4, 2, false).unwrap(),
            "line 4\nline 5\n[File has 5 more lines. Use offset=6 to continue.]"
        );
        assert_eq!(
            file_window(&content, 9, 5, true).unwrap(),
            "9|line 9\n10|line 10"
        );
    }

    #[test]
    fn lines_longer_than_2000_characters_are_cut() {
        let long = "é".repeat(MAX_READ_LINE_CHARS + 5);
        let content = format!("{long}\nshort\n");
        let kept = "é".repeat(MAX_READ_LINE_CHARS);

        assert_eq!(
            file_window(&content, 1, DEFAULT_READ_LINES, false).unwrap(),
            format!("{kept}{TRUNCATED_LINE_MARKER}\nshort\n")
        );
        assert_eq!(
            file_window(&content, 1, DEFAULT_READ_LINES, true).unwrap(),
            format!("1|{kept}{TRUNCATED_LINE_MARKER}\n2|short")
        );
    }

    #[test]
    fn an_offset_past_the_end_explains_itself() {
        assert_eq!(
            file_window("one\ntwo\n", 5, DEFAULT_READ_LINES, true).unwrap(),
            "The file has 2 lines; offset 5 is past the end of the file."
        );
    }

    #[test]
    fn an_empty_file_says_it_exists_but_is_empty() {
        assert_eq!(
            file_window("", 1, DEFAULT_READ_LINES, false).unwrap(),
            "File exists but has empty contents."
        );
    }

    #[test]
    fn a_window_over_100k_characters_is_refused_with_the_paging_hint() {
        let content = format!("{}\n", "x".repeat(1_000)).repeat(150);

        let refusal = file_window(&content, 1, DEFAULT_READ_LINES, false).unwrap_err();

        assert_eq!(
            refusal,
            "File content (~37537 tokens) exceeds the maximum of 25000 tokens. Use the offset and limit parameters to read a specific part of the file, or search it first."
        );
        assert!(file_window(&content, 1, 50, false).is_ok());
    }
}
