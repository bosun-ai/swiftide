use std::borrow::Cow;

use async_trait::async_trait;
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use swiftide_core::{
    AgentContext, Command,
    chat_completion::{Tool, ToolCall, ToolOutput, ToolSpecBuilder, errors::ToolError},
};

/// Replace exact text through the agent's executor (tool name: `edit`).
///
/// A replacement requires one match unless `replace_all` is true. Invalid replacements do not
/// write the file. This is a read/modify/write operation, not a compare-and-swap: callers must
/// coordinate concurrent writers. Filesystem permissions and isolation belong to the executor.
///
/// The tool accepts `file_path`, `old_string`, `new_string`, and optional `replace_all` (false by
/// default). An empty search string or an unchanged replacement
/// returns a failure output.
///
/// # Examples
///
/// ```no_run
/// use swiftide_agents::Agent;
/// use swiftide_agents_contrib::tools::{ExactReplace, ReadFile};
///
/// let mut agent = Agent::builder();
/// agent.add_tool(ReadFile::default()).add_tool(ExactReplace);
/// ```
#[derive(Clone, Debug, Default)]
pub struct ExactReplace;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ExactReplaceArgs {
    /// Path to the file to edit, relative to the repository root.
    file_path: String,
    /// Exact text to replace.
    old_string: String,
    /// Replacement text.
    new_string: String,
    /// Replace every exact match instead of requiring a unique match.
    #[serde(default)]
    replace_all: bool,
}

const TOOL_DESCRIPTION: &str = "Edit a file by replacing exact text. By default, old_string must match exactly once. If it matches zero or multiple times, no changes are written. Set replace_all to true only when every exact match should be replaced.";

#[async_trait]
impl Tool for ExactReplace {
    async fn invoke(
        &self,
        agent_context: &dyn AgentContext,
        tool_call: &ToolCall,
    ) -> Result<ToolOutput, ToolError> {
        let args = tool_call
            .args()
            .ok_or_else(|| ToolError::missing_arguments("edit"))?;
        let payload: ExactReplaceArgs = serde_json::from_str(args)?;
        let contents = agent_context
            .executor()
            .exec_cmd(&Command::read_file(&payload.file_path))
            .await?
            .into_string_lossy();

        let updated = match replace_exact(
            &contents,
            &payload.old_string,
            &payload.new_string,
            payload.replace_all,
        ) {
            Ok(updated) => updated,
            Err(message) => return Ok(ToolOutput::Fail(message)),
        };

        agent_context
            .executor()
            .exec_cmd(&Command::write_file(&payload.file_path, updated))
            .await?;

        Ok(ToolOutput::text(format!(
            "Updated {} successfully",
            payload.file_path
        )))
    }

    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("edit")
    }

    fn tool_spec(&self) -> swiftide_core::chat_completion::ToolSpec {
        ToolSpecBuilder::default()
            .name("edit")
            .description(TOOL_DESCRIPTION)
            .parameters_schema(schema_for!(ExactReplaceArgs))
            .build()
            .expect("tool spec is valid")
    }
}

pub(crate) fn replace_exact(
    contents: &str,
    old_string: &str,
    new_string: &str,
    replace_all: bool,
) -> Result<String, String> {
    if old_string.is_empty() {
        return Err("old_string must not be empty; no changes were written".to_string());
    }
    if old_string == new_string {
        return Err(
            "old_string and new_string must be different; no changes were written".to_string(),
        );
    }

    let matches = contents.matches(old_string).count();
    if matches == 0 {
        return Err(
            "old_string did not match the file exactly; read the file and retry with current text"
                .to_string(),
        );
    }
    if !replace_all && matches != 1 {
        return Err(format!(
            "old_string matched {matches} times; provide more surrounding context for a unique match or set replace_all to true"
        ));
    }

    Ok(if replace_all {
        contents.replace(old_string, new_string)
    } else {
        contents.replacen(old_string, new_string, 1)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use serde_json::json;
    use swiftide_agents::{DefaultContext, tools::local_executor::LocalExecutor};
    use swiftide_core::ToolExecutor;

    #[test]
    fn replaces_one_unique_match() {
        assert_eq!(
            replace_exact("before old after", "old", "new", false),
            Ok("before new after".to_string())
        );
    }

    #[test]
    fn refuses_an_ambiguous_match_without_replace_all() {
        let error = replace_exact("old and old", "old", "new", false)
            .expect_err("ambiguous replacement should fail");

        assert!(error.contains("matched 2 times"));
    }

    #[test]
    fn replace_all_updates_every_match() {
        assert_eq!(
            replace_exact("old and old", "old", "new", true),
            Ok("new and new".to_string())
        );
    }

    #[test]
    fn deletes_a_unique_match_with_an_empty_replacement() {
        assert_eq!(
            replace_exact("before obsolete after", "obsolete ", "", false),
            Ok("before after".to_string())
        );
    }

    #[test]
    fn replaces_exact_multiline_text() {
        assert_eq!(
            replace_exact(
                "before\nold line\nnext line\nafter",
                "old line\nnext line",
                "new line",
                false
            ),
            Ok("before\nnew line\nafter".to_string())
        );
    }

    #[test]
    fn refuses_empty_and_missing_matches() {
        assert!(replace_exact("contents", "", "new", false).is_err());
        assert!(replace_exact("contents", "missing", "new", false).is_err());
    }

    #[test]
    fn refuses_an_identical_replacement() {
        assert!(replace_exact("contents", "contents", "contents", false).is_err());
    }

    #[tokio::test]
    async fn tool_writes_only_after_a_unique_match() {
        let tempdir = temp_dir::TempDir::new().expect("tempdir should exist");
        let path = tempdir.path().join("file.txt");
        std::fs::write(&path, "old and old").expect("fixture should be written");
        let executor: Arc<dyn ToolExecutor> = Arc::new(
            LocalExecutor::builder()
                .workdir(tempdir.path())
                .build()
                .expect("executor should build"),
        );
        let context = DefaultContext::from_executor(executor);
        let tool = ExactReplace;
        let ambiguous_call = ToolCall::builder()
            .id("ambiguous")
            .name("edit")
            .args(
                json!({
                    "file_path": "file.txt",
                    "old_string": "old",
                    "new_string": "new"
                })
                .to_string(),
            )
            .build()
            .expect("tool call should build");

        let output = tool
            .invoke(&context, &ambiguous_call)
            .await
            .expect("ambiguity should be reported to the agent");
        assert!(output.as_fail().is_some());
        assert_eq!(
            std::fs::read_to_string(&path).expect("fixture should be readable"),
            "old and old"
        );

        let replace_all_call = ToolCall::builder()
            .id("replace-all")
            .name("edit")
            .args(
                json!({
                    "file_path": "file.txt",
                    "old_string": "old",
                    "new_string": "new",
                    "replace_all": true
                })
                .to_string(),
            )
            .build()
            .expect("tool call should build");

        tool.invoke(&context, &replace_all_call)
            .await
            .expect("replace_all should succeed");
        assert_eq!(
            std::fs::read_to_string(path).expect("fixture should be readable"),
            "new and new"
        );
    }
}
