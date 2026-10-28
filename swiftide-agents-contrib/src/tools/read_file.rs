use std::{borrow::Cow, num::NonZeroUsize};

use async_trait::async_trait;
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use shell_escape::escape;
use swiftide_core::{
    AgentContext, Command,
    chat_completion::{Tool, ToolCall, ToolOutput, ToolSpec, errors::ToolError},
};

/// Read a regular text file through the agent's executor (tool name: `read_file`).
///
/// Reads default to 2,000 lines with a 100,000-byte text budget, following Fluyt's paged reader.
/// Optional `start_line` and `end_line` select a one-based, inclusive range and include original
/// line numbers as `number|text`. Omit `start_line` to begin at line one; omit `end_line` for the
/// default page size. Explicit ranges may request up to 100,000 lines but must fit the text budget.
/// Partial default pages include the next `start_line`; oversized explicit ranges fail rather than
/// silently returning incomplete text. An oversized single line fails. Empty files and ranges past
/// the end return empty text. Numbered output ends each returned line with a newline.
///
/// Use [`Self::with_line_numbers`] to number default pages too. Both modes accept paths relative
/// to the executor's working directory or absolute paths in its filesystem. Ranges and numbered
/// reads require a POSIX-like shell with `tail` and `head`. The executor selects the page and
/// caps returned file bytes before formatting. The executor controls access and isolation.
///
/// # Examples
///
/// ```no_run
/// use swiftide_agents::Agent;
/// use swiftide_agents_contrib::tools::ReadFile;
///
/// let mut agent = Agent::builder();
/// agent.add_tool(ReadFile::default())
///     .add_tool(ReadFile::default().with_line_numbers());
/// ```
///
/// For example, `{"file_name":"src/lib.rs","start_line":2,"end_line":3}` returns lines two
/// and three, with their original numbers. Zero line numbers fail argument parsing. An end before
/// the start returns a failure output without accessing the file. File and executor errors remain
/// tool errors so the agent can handle them.
#[derive(Clone, Debug, Default)]
pub struct ReadFile {
    numbered: bool,
}

impl ReadFile {
    /// Number default pages and use the tool name `read_file_with_line_numbers`.
    ///
    /// Range reads include line numbers in either mode. This setting also adds numbers when the
    /// caller omits both range arguments.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents::Agent;
    /// use swiftide_agents_contrib::tools::ReadFile;
    ///
    /// // Give the agent original line numbers to use when discussing file changes.
    /// Agent::builder().add_tool(ReadFile::default().with_line_numbers());
    /// ```
    #[must_use]
    pub fn with_line_numbers(mut self) -> Self {
        self.numbered = true;
        self
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReadArgs {
    /// Path in the executor's filesystem, relative to its working directory or absolute.
    file_name: String,
    /// First line to read, starting at one. Omit to begin at line one.
    start_line: Option<NonZeroUsize>,
    /// Last line to read, inclusive. Omit for a page of up to 2,000 lines.
    end_line: Option<NonZeroUsize>,
}

const DEFAULT_LINE_LIMIT: usize = 2_000;
const MAX_LINE_LIMIT: usize = 100_000;
const MAX_TEXT_BYTES: usize = 100_000;

impl ReadArgs {
    fn start(&self) -> usize {
        self.start_line.map_or(1, NonZeroUsize::get)
    }

    fn line_limit(&self) -> usize {
        self.end_line
            .map_or(DEFAULT_LINE_LIMIT, |end| end.get() - self.start() + 1)
    }

    fn numbered(&self, numbered: bool) -> bool {
        numbered || self.start_line.is_some() || self.end_line.is_some()
    }

    fn command(&self) -> Command {
        let start = self.start();
        let lines = self.line_limit() + 1;
        let bytes = MAX_TEXT_BYTES + 1;
        let path = escape(Cow::Borrowed(self.file_name.as_str()));
        // Open before the pipeline so missing files remain executor errors. The extra line/byte
        // detects partial pages while keeping large files and long lines out of returned output.
        Command::shell(format!(
            "{{ if [ ! -f {path} ]; then printf '%s\\n' 'read_file requires a regular file' >&2; exit 1; fi; tail -n +{start} | head -n {lines} | head -c {bytes}; }} < {path}"
        ))
    }

    fn render(&self, bytes: &[u8], numbered: bool) -> ToolOutput {
        let cut = bytes.len() > MAX_TEXT_BYTES;
        let text = String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_TEXT_BYTES)]);
        let numbered = self.numbered(numbered);
        let mut rendered = String::new();
        let mut emitted = 0;
        let mut lines = text.split_inclusive('\n');
        for (index, line) in lines.by_ref().take(self.line_limit()).enumerate() {
            // A byte cap may cut the final line; never present that fragment as a complete line.
            if cut && !line.ends_with('\n') {
                return self.overflow(rendered, emitted);
            }
            let line_number = self.start() + index;
            let formatted = if numbered {
                Cow::Owned(format!(
                    "{line_number}|{}\n",
                    line.strip_suffix('\n').unwrap_or(line)
                ))
            } else {
                Cow::Borrowed(line)
            };
            if rendered.len() + formatted.len() > MAX_TEXT_BYTES {
                return self.overflow(rendered, emitted);
            }
            rendered.push_str(&formatted);
            emitted += 1;
        }
        if self.end_line.is_some() && emitted == self.line_limit() {
            return ToolOutput::text(rendered);
        }
        if lines.next().is_some() {
            return self.partial(rendered, emitted);
        }
        if cut {
            return self.overflow(rendered, emitted);
        }
        ToolOutput::text(rendered)
    }

    fn overflow(&self, rendered: String, emitted: usize) -> ToolOutput {
        if self.start_line.is_some() || self.end_line.is_some() || emitted == 0 {
            ToolOutput::fail(
                "The file or requested range exceeds the 100,000-byte output budget; use a smaller range or search for specific content.",
            )
        } else {
            self.partial(rendered, emitted)
        }
    }

    fn partial(&self, mut rendered: String, emitted: usize) -> ToolOutput {
        use std::fmt::Write as _;
        let next = self.start().saturating_add(emitted);
        write!(rendered, "\n[PARTIAL view: showing lines {}-{}. Continue with start_line={next} and a smaller end_line.]", self.start(), next.saturating_sub(1)).expect("string write");
        ToolOutput::text(rendered)
    }
}

#[async_trait]
impl Tool for ReadFile {
    async fn invoke(
        &self,
        context: &dyn AgentContext,
        call: &ToolCall,
    ) -> Result<ToolOutput, ToolError> {
        let args = call
            .args()
            .ok_or_else(|| ToolError::missing_arguments(self.name().into_owned()))?;
        let args: ReadArgs = serde_json::from_str(args)?;
        if args.end_line.is_some_and(|end| end.get() < args.start()) {
            return Ok(ToolOutput::fail("end_line must be at or after start_line"));
        }
        if args.line_limit() > MAX_LINE_LIMIT {
            return Ok(ToolOutput::fail(
                "Request at most 100,000 lines per read; use multiple reads",
            ));
        }
        let output = context.executor().exec_cmd(&args.command()).await?;
        Ok(args.render(output.as_bytes().as_ref(), self.numbered))
    }

    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed(if self.numbered {
            "read_file_with_line_numbers"
        } else {
            "read_file"
        })
    }

    fn tool_spec(&self) -> ToolSpec {
        ToolSpec::builder()
            .name(self.name())
            .description(if self.numbered {
                "Read a file with line numbers. Optional start_line and end_line select a one-based, inclusive range. Defaults to 2,000 lines and a 100,000-byte text budget; partial pages include continuation instructions."
            } else {
                "Read a file. Optional start_line and end_line select a one-based, inclusive range with original line numbers. Defaults to 2,000 lines without line numbers and a 100,000-byte text budget; partial pages include continuation instructions."
            })
            .parameters_schema(schema_for!(ReadArgs))
            .build()
            .expect("valid file-reading schema")
    }
}
