# swiftide-agents-contrib

Optional tools, context hooks, and executor helpers for Swiftide agents, extracted from Fluyt.
Choose features and register the components your agent needs. No features are enabled by default.
See [source parity](https://github.com/bosun-ai/swiftide/blob/master/swiftide-agents-contrib/SOURCE_PARITY.md) for the source revision and extraction details.

```toml
[dependencies]
swiftide-agents = "0.33"
swiftide-core = "0.33"
swiftide-agents-contrib = { version = "0.33", features = ["planning", "file-tools"] }
```

| Feature | Components | Requirements |
| --- | --- | --- |
| `file-tools` | `ReadFile`, `ExactReplace`, `ApplyPatch` | An executor with a POSIX-like shell and the source tools' `cat`, `mkdir`, and `rm` commands |
| `planning` | `UpdatePlan`, `PlanMemory`, `PlanItem`, `PlanStatus` | In-process memory |
| `context-management` | `ToolSummarizer`, `ContextCompression`, and builders | Caller-supplied models and Git context; examples also use `anyhow = "1"` |
| `lazy-executor` | `LazyExecutor`, `ToolExecutorExt` | An async executor factory; downloads use `dd`, `base64`, and `sha256sum` |

Through the main crate, enable `agents-contrib-file-tools`, `agents-contrib-planning`,
`agents-contrib-context`, or `agents-contrib-lazy-executor`, and import from
`swiftide::agents_contrib`. The `agents-contrib` feature alone exposes the crate without
capabilities. Swiftide's default and `all` feature selections are unchanged.

## File tools and plans

```no_run
# #[cfg(all(feature = "planning", feature = "file-tools"))]
# {
use swiftide_agents::Agent;
use swiftide_agents_contrib::tools::{ApplyPatch, ExactReplace, ReadFile, UpdatePlan};

Agent::builder()
    .add_tool(ReadFile::default())
    .add_tool(ExactReplace)
    .add_tool(ApplyPatch)
    .add_tool(UpdatePlan::default());
// Configure your model and executor, then build and run the agent.
# }
```

| Tool name | Arguments | Behavior |
| --- | --- | --- |
| `read_file` | `file_name`, optional `offset` and `limit` | Defaults to 2,000 lines. Zero offset starts at one; zero limit fails. |
| `read_file_with_line_numbers` | Same arguments | Same reader and caps, with original `N|` line numbers. |
| `edit` | `file_path`, `old_string`, `new_string`, optional `replace_all` | Replaces a unique exact match, or all matches when requested. |
| `apply_patch` | `patch` | Adds, updates, moves, and deletes files using the format below. |
| `update_plan` | `items` and `explanation`, both required | Stores steps with `pending`, `in_progress`, or `completed` status. Empty items retrieve the plan. |

`ReadFile::default()` registers `read_file`; `ReadFile::default().with_line_numbers()` registers
`read_file_with_line_numbers`. Both use Fluyt's shared reader unchanged. They accept relative or
absolute paths, cut lines at 2,000 characters with `… [line truncated]`, and refuse a selected
window over 100,000 characters using the source accounting. A partial page explains the next
offset. Files below all limits retain their original text in plain mode.

```json
{"file_name":"src/lib.rs","offset":2001,"limit":1000}
```

The executor reads the complete file before selecting and rendering the page, matching Fluyt.
Nonzero command exits are forwarded as text. The separate provider-specific `Read` tool and its
edit-state tracking are not exposed here.

`UpdatePlan` rejects empty steps and multiple in-progress steps without replacing the plan.
Clones share state; fresh default tools have independent plans. State is not persisted.
Supply an empty `explanation` string when there is nothing to explain.

```text
*** Begin Patch
*** Add File: hello.txt
+Hello
*** Update File: src/main.rs
@@ fn greet()
-    println!("Hi");
+    println!("Hello");
*** End Patch
```

Moves use `*** Move to: new/path` after an update header; deletes use `*** Delete File: path`.
Unchanged lines start with a space, removals with `-`, and additions with `+`.
Patches execute in order and can partly succeed; inspect the result report. The executor controls
filesystem access and isolation. These tools retain the source's lossy text decoding and patch
newline handling; they do not add file identity checks or transaction guarantees.

## Context hooks

```no_run
# #[cfg(feature = "context-management")]
# {
use std::sync::Arc;
use swiftide_agents::AgentBuilder;
use swiftide_agents_contrib::hooks::{ContextCompression, ToolSummarizer};
use swiftide_core::{ChatCompletion, SimplePrompt};

fn attach(
    agent: &mut AgentBuilder,
    output_model: Arc<dyn SimplePrompt>,
    conversation_model: Arc<dyn ChatCompletion>,
) -> anyhow::Result<()> {
    let output = ToolSummarizer::builder()
        .llm(output_model)
        .tools_to_summarize(Vec::new())
        .git_start_sha("HEAD")
        .build()?;
    let conversation = ContextCompression::builder()
        .llm(conversation_model)
        .git_start_sha("HEAD")
        .git_diff_dir(".")
        .context_limit(32_000usize)
        .build()?;
    agent.after_tool(output.summarize_hook());
    agent.after_each(conversation.summarize_hook());
    Ok(())
}
# }
```

`ToolSummarizer` truncates at an estimated 10,000 tokens, retaining 40 head lines and 160 tail
lines. Automatic model summarization is disabled by default; use `summarize_all_at_token_cutoff`
to enable it, or list tools in `tools_to_summarize`. File-read tools are excluded by default;
explicit inclusion takes precedence. The original prompts include Git diffs for test and coverage
tools. Model errors propagate, and empty replies replace output with empty text.

`ContextCompression` retains the source's turn counter, history filtering, prompts, Git diff,
plan extraction, and plain-text original-instructions recovery tool. Its default turn threshold
is five, checked before incrementing, so the sixth eligible hook call triggers a summary. Recent
file reads postpone compression below 90% of the context limit. The default context limit is
400,000,000 with a 0.7 cutoff; set a limit suitable for your model, as in the example.
The counter resets before a summary attempt, including a failed or empty attempt.

Use fresh hook instances for independent agents. Summarization is lossy. Source diagnostics can
log prompts and summaries at debug level. No new final-answer guards, retry policies, token
estimator configuration, or multipart recovery behavior are added by this extraction.

## Lazy executor

```no_run
# #[cfg(feature = "lazy-executor")]
# {
use std::sync::Arc;
use swiftide_agents::tools::local_executor::LocalExecutor;
use swiftide_agents_contrib::executors::{LazyExecutor, ToolExecutorExt};

let executor = LazyExecutor::new(|| async {
    Ok(Arc::new(LocalExecutor::default()) as Arc<dyn ToolExecutorExt>)
});
assert!(!executor.is_started());
# }
```

Commands, streams, and downloads initialize the executor on demand. Clones share initialization;
failed initialization can retry. `ToolExecutorExt` preserves startup status, cleanup, and download
delegation. Cleanup before startup does nothing. Downloads retain the source's size limit,
chunking, checksum verification, and removal of partial local files on failure.

## Verification

```sh
cargo run -p swiftide-agents-contrib --example coding_tools --features planning,file-tools
cargo run -p swiftide-agents-contrib --example context_hooks --features context-management
cargo test -p swiftide-agents-contrib --all-features
cargo test -p swiftide-agents-contrib --doc --all-features
```

Tests use synthetic files and offline models. Behavioral changes should be made in the source
implementation first, then carried over with matching tests.
