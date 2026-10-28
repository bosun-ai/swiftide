//! File-oriented patch parsing, matching, application, and result reporting.
use std::borrow::Cow;

use anyhow::anyhow;
use async_trait::async_trait;
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use swiftide_core::{
    AgentContext,
    chat_completion::Tool,
    chat_completion::{ToolCall, ToolOutput, ToolSpecBuilder, errors::ToolError},
};

/// Apply file-oriented patches through the agent's executor.
///
/// Supports add, update, move and delete operations with named hunk anchors. Paths must be
/// relative and cannot contain parent directory components. Operations execute in order; a
/// patch is not a transaction. Read the returned report to see which operations succeeded.
/// The executor controls filesystem access, including symlinks and isolation. Shell operations
/// require a POSIX-like shell with `mkdir` and `rm` supporting `--` and file identity tests
/// (`-ef`). Moves onto the same file through symlinks or hard links are rejected before writing.
/// Coordinate concurrent filesystem changes in the executor; path checks and writes are separate
/// operations.
///
/// The tool name is `apply_patch`. Its `patch` argument uses file headers and line-based hunks:
///
/// ```text
/// *** Begin Patch
/// *** Update File: greeting.txt
/// @@
/// -Hello
/// +Hello, world!
/// *** End Patch
/// ```
///
/// # Examples
///
/// ```no_run
/// use swiftide_agents::Agent;
/// use swiftide_agents_contrib::tools::ApplyPatch;
///
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

mod execution;
mod matching;
mod parser;
mod report;
use execution::apply_hunk;
use parser::parse_patch;
use report::ApplyPatchSummary;
