use std::{borrow::Cow, num::NonZeroUsize};

use async_trait::async_trait;
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use shell_escape::escape;
use swiftide_core::{
    AgentContext, Command,
    chat_completion::{Tool, ToolCall, ToolOutput, ToolSpec, errors::ToolError},
};

/// Read a text file through the agent's executor (tool name: `read_file`).
///
/// With only `file_name`, return the full contents without adding line numbers or applying an
/// output limit. Optional `start_line` and `end_line` select a one-based, inclusive range and
/// include the original line numbers as `number|text`. Omit `start_line` to begin at line one;
/// omit `end_line` to read through the end. An empty file or a range past its end returns empty
/// text. Numbered output ends each returned line with a newline, including the final line.
///
/// Use [`Self::with_line_numbers`] to number full-file reads too. Both modes accept paths relative
/// to the executor's working directory or absolute paths in its filesystem. Ranges and numbered
/// reads require a POSIX-like shell with `awk`; the executor selects the lines before returning
/// output. Full-file reads use [`Command::read_file`]. The executor controls access and isolation.
///
/// # Example
///
/// ```
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
    /// Number full-file reads and use the tool name `read_file_with_line_numbers`.
    ///
    /// Range reads include line numbers in either mode. This setting adds numbers when the caller
    /// omits both range arguments, preserving the existing numbered-file tool contract.
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
    /// Last line to read, inclusive. Omit to read through the end of the file.
    end_line: Option<NonZeroUsize>,
}

impl ReadArgs {
    fn start(&self) -> usize {
        self.start_line.map_or(1, NonZeroUsize::get)
    }

    fn command(&self, numbered: bool) -> Command {
        if !numbered && self.start_line.is_none() && self.end_line.is_none() {
            return Command::read_file(&self.file_name);
        }
        let start = self.start();
        let end = self.end_line.map_or(0, NonZeroUsize::get);
        let path = escape(Cow::Borrowed(self.file_name.as_str()));
        // Redirection keeps filenames out of awk's option and variable-assignment parsing.
        Command::shell(format!(
            "awk -v start={start} -v end={end} 'NR >= start {{ printf \"%d|%s\\n\", NR, $0 }} end > 0 && NR >= end {{ exit }}' < {path}"
        ))
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
        Ok(context
            .executor()
            .exec_cmd(&args.command(self.numbered))
            .await?
            .into())
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
                "Read a file with line numbers. Optional start_line and end_line select a one-based, inclusive range. Omit both for the full file."
            } else {
                "Read a file. Optional start_line and end_line select a one-based, inclusive range with original line numbers. Omit both for the full file without line numbers."
            })
            .parameters_schema(schema_for!(ReadArgs))
            .build()
            .expect("valid file-reading schema")
    }
}
