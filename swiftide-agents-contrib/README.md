# swiftide-agents-contrib

Optional tools, context hooks, and executor helpers for Swiftide agents.
Every capability is feature flagged and must be explicitly installed on an agent.

## Installation

```toml
[dependencies]
swiftide-agents = "0.33"
swiftide-agents-contrib = { version = "0.33", features = ["planning", "file-tools"] }
```

| Feature | Components | Runtime requirements |
| --- | --- | --- |
| `context-management` | `hooks::{ToolSummarizer, ContextCompression}` and builders | Caller-supplied summarization models; truncation alone needs no model |
| `planning` | `tools::{UpdatePlan, PlanMemory, PlanItem, PlanStatus}` | In-process memory |
| `file-tools` | `tools::{ReadFile, ExactReplace, ApplyPatch}` | Agent executor; numbered reads use `awk`; patches use `mkdir` and `rm` supporting `--` in a POSIX-like shell |
| `lazy-executor` | `executors::LazyExecutor` | Caller-supplied async executor factory |

The lazy-executor example also needs `swiftide-core = "0.33"` in your dependencies.
The context-hook examples use `anyhow = "1"` for error handling; compression examples that
import `swiftide_core::ChatCompletion` also need `swiftide-core = "0.33"`.

There are no default features, provider SDKs, or tokenizer dependencies.
Alternatively, use `swiftide::agents_contrib` through the main crate:

```toml
[dependencies]
swiftide = { version = "0.33", default-features = false, features = ["agents-contrib-planning", "agents-contrib-file-tools"] }
```

Facade features `agents-contrib-context`, `agents-contrib-planning`, `agents-contrib-file-tools`,
and `agents-contrib-lazy-executor` forward the respective capabilities. `agents-contrib` alone
exposes the crate and Swiftide agents without contrib capabilities. Swiftide's default and `all`
feature selections remain unchanged.

## File tools and plans

```no_run
# #[cfg(all(feature = "planning", feature = "file-tools"))]
# {
use std::sync::Arc;
use swiftide_agents::Agent;
use swiftide_agents_contrib::tools::{ApplyPatch, ExactReplace, PlanMemory, ReadFile, UpdatePlan};

let memory = Arc::new(PlanMemory::default());
let mut agent = Agent::builder();
agent.add_tool(UpdatePlan::new(Arc::clone(&memory)))
    .add_tool(ReadFile::default())
    .add_tool(ExactReplace)
    .add_tool(ApplyPatch);
assert!(memory.snapshot().is_empty());
// Configure the model and executor, then build and run the agent.
# }
```

`ReadFile::default()` registers `read_file`. Call it with `file_name` to read the full file
without adding line numbers or applying an output limit. To read an excerpt, supply
`start_line`, `end_line`, or both. Line numbers start at one and the end is inclusive:

```json
{"file_name":"src/lib.rs","start_line":2,"end_line":3}
```

Excerpts include their original line numbers as `2|text`. Omit the start to begin at line one;
omit the end to read through the file's end. Empty files and ranges past the end return empty
text. Zero line numbers fail argument parsing; an end before the start returns a failure before
accessing the file. File and executor errors remain visible to the agent.

Use `ReadFile::default().with_line_numbers()` to register `read_file_with_line_numbers`, which
numbers full-file reads too. Numbered output ends each line with a newline. Both tools accept
relative paths within the executor's working directory and absolute paths in its filesystem.
The executor selects excerpts before returning output, using a POSIX-like shell and `awk`.
Full-file reads use the executor's file-reading command. Exclude both reading tool names from
`ToolSummarizer` if your agent needs to retain their full output.

`update_plan` accepts `items` (each with a `step` and `pending`, `in_progress`, or `completed`
status) and an optional `explanation`. Empty items retrieve the current plan. Invalid steps or
multiple in-progress steps return a failure without changing state. Clones share state; fresh
default tools have independent plans. Plans are not automatically persisted.

`edit` accepts `file_path`, `old_string`, `new_string`, and optional `replace_all` (default false).
It requires a unique exact match unless replacing all matches. Rejected replacements never write.
Files must contain valid UTF-8. Reads and writes use the executor; applications must coordinate
concurrent writers.

`apply_patch` accepts a `patch` string:

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

Updates support `*** Move to: new/path` immediately after the file header; deletes use
`*** Delete File: path`. Consecutive `@@` anchors navigate nested/repeated text. Unchanged lines
start with a space, removals with `-`, and additions with `+`. Ambiguous matches fail; already
applied updates are recognized on retry. UTF-8 files retain CRLF endings and trailing blank lines;
non-UTF-8 updates fail before writing.

The full patch is parsed before execution. Operations then run in order and can partially succeed;
inspect the Applied / Already present / Not applied report. A failed move can leave the destination
written and source unchanged; this is reported explicitly. Patches are not transactions. Paths
must be relative, nonempty, and free of parent-directory components. The executor controls access,
symlinks, isolation, and command execution. These tools do not add a sandbox.

## Context hooks

```no_run
# #[cfg(feature = "context-management")]
# {
use swiftide_agents::Agent;
use swiftide_agents_contrib::hooks::ToolSummarizer;

let output = ToolSummarizer::builder()
    .max_output_chars(40_000)
    .excluded_tools(["read_file", "read_file_with_line_numbers"])
    .build()?;
let mut agent = Agent::builder();
agent.after_tool(output.summarize_hook());
# }
# Ok::<(), anyhow::Error>(())
```

Output truncation prefers head/tail lines and handles Unicode and huge single lines. Defaults:
40,000 characters, 40 head lines, 160 tail lines. With `.llm(Arc<dyn SimplePrompt>)`, outputs
exceeding 5,000 estimated tokens after truncation are summarized. `.tools_to_summarize(...)`
forces selected tools; `.excluded_tools(...)` overrides both operations. Text and failure variants
are preserved; stop/approval outputs and executor errors are untouched. Failed/empty model replies
retain truncated output. Generated summaries are also subject to the character limit.

Build `ContextCompression::builder().llm(Arc<dyn ChatCompletion>)` and register
`.summarize_hook()` with `after_each`. The default trigger is five completed turns. Add
`.context_limit(tokens)` to trigger above an estimated token count; leave room for system prompts,
tool schemas, media, and the response. `.summarize_every_turns(0)` disables the turn trigger;
at least one trigger must remain enabled. Stopped agents and final answers without tool calls are
skipped. Use a fresh hook for each independent agent; cloned closures share counters and original
instructions.

Both builders accept `.token_estimator(...)` using Swiftide's existing `EstimateTokens` trait,
with `CharEstimator` as default. Applications can supply an existing `TikToken` instance. Counts
are estimates and cannot guarantee a request fits a model. Both accept a custom `Prompt` with
application context: output prompts receive `tool_name`, `tool_args`, and `tool_output`, while
conversation prompts receive `original_instructions`. Git and plan state are not required.

Compression combines the latest summary with newer exchanges. Tool calls/results become explicit
data for the summarizer, without orphan tool calls. Successful summaries use Swiftide's existing
summary messages followed by a user continuation; full history remains available. The reserved
`read_original_instructions` tool recovers the first user message and can reattach multipart
instructions with media after all tool results have been recorded. Failed/empty responses preserve
history and the turn counter for retry.
Summarization is lossy; store essential facts separately. Diagnostics do not log raw content.

## Lazy executor

```no_run
# #[cfg(feature = "lazy-executor")]
# {
use std::sync::Arc;
use swiftide_core::ToolExecutor;
use swiftide_agents::tools::local_executor::LocalExecutor;
use swiftide_agents_contrib::executors::LazyExecutor;

let executor = LazyExecutor::new(|| async {
    Ok(Arc::new(LocalExecutor::default()) as Arc<dyn ToolExecutor>)
});
assert!(!executor.is_initialized());
# }
```

The first command or file stream initializes the executor; clones and concurrent calls share the
result. Failed/cancelled initialization can retry. Factories must tolerate retries and clean up
partial resources. Output streams are forwarded directly. Resource cleanup remains the
application's responsibility because `ToolExecutor` has no cleanup lifecycle.

## Offline examples and tests

```sh
cargo run -p swiftide-agents-contrib --example coding_tools --features planning,file-tools
cargo run -p swiftide-agents-contrib --example context_hooks --features context-management
cargo test -p swiftide-agents-contrib --all-features
cargo test -p swiftide-agents-contrib --doc --all-features
```

The patch and exact-replacement algorithms originated in Bosun's internal agent tooling. Tests
here use synthetic examples rather than production repositories or run data.
