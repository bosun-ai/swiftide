# Output

Real output of `ask-your-codebase` on the `swiftide-agents` crate of this repository, captured
with the debug build (`cargo build -p ask-your-codebase`) on 2026-11-05.

- **Endpoint:** an OpenAI-compatible gateway at `http://127.0.0.1:8080/v1` (Chat Completions;
  the gateway forwards to Anthropic models on AWS Bedrock).
- **Models:** `claude-haiku-5-5` for the per-chunk questions while indexing, `claude-sonnet-5-5`
  for answers.
- **Embeddings:** FastEmbed `jina-embeddings-v2-base-code`, locally on the CPU.

All commands run from the repository root with:

```bash
export OPENAI_BASE_URL=http://127.0.0.1:8080/v1
export OPENAI_API_KEY=<gateway key>
```

## Index from scratch

```console
$ rm -rf swiftide-agents/.ask-your-codebase
$ ./target/debug/ask-your-codebase index swiftide-agents --model claude-haiku-5-5
Indexing Rust files in swiftide-agents ...
Indexed 34 chunks from 14 new or changed files (14 files found, 0 unchanged) in 91.9s
```

14 files are found: the 16 Rust files under `swiftide-agents/` minus `src/test_utils.rs` and the
one file in `tests/`. Unit tests are cut out before chunking.

## Index again: nothing changed

```console
$ ./target/debug/ask-your-codebase index swiftide-agents --model claude-haiku-5-5
Indexing Rust files in swiftide-agents ...
Indexed 0 chunks from 0 new or changed files (14 files found, 14 unchanged) in 0.6s
```

The `Redb` node cache skips every unchanged file, so no prompts are sent and nothing is embedded.

## How does an agent decide to stop?

````console
$ ./target/debug/ask-your-codebase ask --model claude-sonnet-5-5 --path swiftide-agents "How does an agent decide to stop?"
# How an agent decides to stop

An agent stops when something calls `Agent::stop` (or `stop_with_error`), which moves the state to `State::Stopped(StopReason)`. The main loop in `run_agent` checks that state after each step. `Agent::stop` is a no-op if the agent is already stopped (`src/agent.rs`).

## The stop mechanism

- `Agent::stop(reason)` fires the `OnStop` hooks with the reason, then sets `self.state = State::Stopped(reason)`. It only takes effect at the end of the current loop iteration (`src/agent.rs`).
- `Agent::stop_with_error(&AgentError)` fires `OnStop` with `StopReason::Error` and the error, then sets `Stopped(StopReason::Error)` (`src/agent.rs`).
- You can inspect the result with `stop_reason()`, `is_stopped()` and `state()` (`src/agent.rs`, `src/state.rs`).

## Triggers, by `StopReason` variant (`src/state.rs`)

| Variant | When it applies |
|---|---|
| `RequestedByTool(ToolCall, Option<Value>)` | A tool returned `ToolOutput::Stop`. |
| `FeedbackRequired { tool_call, payload }` | A tool returned `ToolOutput::FeedbackRequired`. |
| `AgentFailed(Option<Value>)` | A tool returned `ToolOutput::AgentFailed`. |
| `ToolCallsOverLimit(ToolCall)` | The same tool call failed too many times. |
| `Error` | `step` returned an `Err`. |
| `NoNewMessages` | The completion loop ended without another reason. |
| `Other(String)` | Anything convertible from `AsRef<str>`, e.g. `agent.stop("done")`. |

### Tool-driven stops

After each tool runs in `invoke_tools`, `handle_control_tools` matches the `ToolOutput` and calls `self.stop(...)` with the matching reason (`src/agent.rs`). The built-in control tools produce these outputs (`src/tools/control.rs`):

- **`Stop`** is named `"stop"` and returns `ToolOutput::stop()`. It is the only default tool (`Agent::default_tools()`), so every agent can stop itself. `AgentBuilder::without_default_stop_tool()` removes it.
- **`StopWithArgs`** also uses the name `"stop"`, so it overrides the default. It returns `ToolOutput::stop_with_args(output)`, which carries a payload that ends up in `RequestedByTool`'s `Option<Value>`.
- **`AgentCanFail`** is named `"task_failed"` and returns `ToolOutput::agent_failed(reason)`. This stops the agent with `AgentFailed` rather than returning an `Err`.
- **`ApprovalRequired`** wraps another tool. Unless the context has already received `ToolFeedback::Approved` for that call, it returns `ToolOutput::FeedbackRequired(None)`, which stops the agent with `FeedbackRequired`. In that case the `ToolOutput` message is not added to history, so the tool call stays open. A later run calls `invoke_pending_tool_calls` to resume it (`src/agent.rs`).

### Repeated tool failures

When a tool returns `Err`, `invoke_tools` records a failed `ToolOutput` in history. It then calls `tool_calls_over_limit`, which counts failures per hash of the tool call (name and args) against `tool_retry_limit` (default 3). Once the limit is exceeded, the agent calls `stop(StopReason::ToolCallsOverLimit(..))` and returns the error. The counter is not reset when the agent stops (`src/agent.rs`).

### Loop-level stops in `run_agent` (`src/agent.rs`)

The loop is `while let Some(messages) = context.next_completion()`:

1. **`step` fails.** `stop_with_error` is called (reason `Error`) and `run_agent` returns the `Err`.
2. **`just_once` is set** (`query_once` / `run_once`), **or the state is already stopped** (a tool stop, for example). The loop `break`s after the step.
3. **The optional `limit` is reached** (`loop_counter >= limit`). It logs "Agent loop limit reached" and breaks.
4. **`next_completion()` returns `None`**, meaning there are no new messages. The loop ends.

After the loop, `run_agent` always calls `self.stop(StopReason::NoNewMessages)`. If a tool or error already stopped the agent, that call does nothing. Otherwise (limit reached, `just_once`, or no more messages) the reason becomes `NoNewMessages`.

## Not shown in the snippets

The condition under which `next_completion()` returns `None` isn't included. `DefaultContext` has a `stop_on_assistant: true` field ("Stop if last message is from the assistant"), which suggests an assistant reply without tool calls ends the loop. The implementation isn't shown, so that is an inference.

## Observing a stop

Register `AgentBuilder::on_stop(|agent, reason, err| ...)`, which receives the `StopReason` and an optional `&AgentError` (`src/hooks.rs`). After the run, use `agent.stop_reason()` (`src/agent.rs`).

Sources:
  - src/agent.rs
  - src/tools/control.rs
  - src/lib.rs
  - src/state.rs
  - src/errors.rs
  - src/task_agent.rs
  - src/hooks.rs
  - src/tasks.rs
  - src/default_context.rs
````

## What hooks can I register and when do they run?

````console
$ ./target/debug/ask-your-codebase ask --model claude-sonnet-5-5 --path swiftide-agents "What hooks can I register and when do they run?"
# Agent hooks

## Registering hooks

Hooks are variants of the `Hook` enum (`src/hooks.rs`). Each wraps a boxed closure that returns a pinned, boxed async block (`Pin<Box<dyn Future<Output = Result<()>> + Send>>`).

On `AgentBuilder` you can use one of the typed helpers, or `add_hook(Hook)` / `hooks(Vec<Hook>)` (`src/agent.rs`). `hooks(...)` replaces the whole list.

```rust
Agent::builder()
    .before_completion(|_agent, _req| Box::pin(async move { Ok(()) }))
    .on_stop(|_agent, reason, err| Box::pin(async move { Ok(()) }));
```

## Hook types

| Builder method | `Hook` variant | Closure args | Runs |
|---|---|---|---|
| `before_all` | `BeforeAll` | `&mut Agent` | Once, when the agent leaves the pending state. It does not fire again after pause or resume. |
| `on_start` | `OnStart` | `&mut Agent` | Every time the agent starts, including a restart from stopped. It runs after `BeforeAll` and before `BeforeCompletion`. |
| `before_completion` | `BeforeCompletion` | `&mut Agent`, `&mut ChatCompletionRequest` | Before each LLM call. |
| `on_stream` | `OnStream` | `&Agent`, `&ChatCompletionResponse` | For each streamed chunk. The response holds the accumulated message and the delta. Registering it sets `streaming = true`. |
| `after_completion` | `AfterCompletion` | `&mut Agent`, `&mut ChatCompletionResponse` | After each completion, before tool invocation and before new messages are added. |
| `before_tool` | `BeforeTool` | `&mut Agent`, `&ToolCall` | Before each tool call. |
| `after_tool` | `AfterTool` | `&mut Agent`, `&ToolCall`, `&mut Result<ToolOutput, ToolError>` | After each tool call. The result is mutable, so you can rewrite the output or error. |
| `after_each` | `AfterEach` | `&mut Agent` | After the completion and its tool calls, right before a new loop may start. |
| `on_new_message` | `OnNewMessage` | `&Agent`, `&mut ChatMessage` | When the agent adds a message to the context. Each tool output is a separate message. |
| `on_stop` | `OnStop` | `&mut Agent`, `StopReason`, `Option<&AgentError>` | When the agent stops. |

## Order of execution (`run_agent` and `step` in `src/agent.rs`)

1. If the agent is pending, `BeforeAll` runs. After it, the system prompt is added and toolboxes are loaded.
2. Any queued query is stored, and pending tool calls from earlier history are invoked. This can trigger `BeforeTool` and `AfterTool`. Pending user messages are then flushed.
3. `OnStart` runs, and the state becomes `Running`.
4. For each loop iteration:
   - If the history has tool calls without outputs, the tools are invoked and the loop `continue`s. No `step` runs, so there are no completion hooks.
   - Otherwise `step` runs:
     1. `BeforeCompletion` runs.
     2. The LLM is called. When streaming, `OnStream` runs for each chunk.
     3. `AfterCompletion` runs.
     4. Reasoning and assistant messages are added through `Agent::add_message`, which fires `OnNewMessage`.
     5. Tools are invoked (see below).
     6. `AfterEach` runs.
5. When the loop ends, `stop(StopReason::NoNewMessages)` is called.

### Tool invocation (`invoke_tools`)

- `BeforeTool` runs for each tool call before the tool is spawned. Tools that aren't found are skipped, so no hook runs for them.
- After the spawned tasks are joined, `AfterTool` runs for each call, before the error check. A hook can therefore alter a failed result.
- The `ToolOutput` message is then added, which fires `OnNewMessage`.

### Stopping

- `Agent::stop` invokes `OnStop` with the reason and `None`. This happens for the stop tool, feedback required, agent failed, tool retries over the limit, and no new messages.
- `stop_with_error` invokes `OnStop` with `StopReason::Error` and `Some(error)`. It is called when `step` fails.
- Both are no-ops, and fire no hook, if the agent is already stopped.

## Execution semantics (`src/util.rs`)

- The `invoke_hooks!` macro runs all hooks of a type in registration order. They run sequentially and are awaited one at a time.
- An `Err` from a hook is only logged with `tracing::error!`. It is not propagated and does not stop the agent or the other hooks.
- Each call is wrapped in a tracing span. Most hook types also log a `debug` line, but `OnStream` uses only a `trace` span, to keep streaming quiet.
- `OnNewMessage` fires only for messages added via `Agent::add_message`. Adding messages directly through `agent.context()` bypasses it (`src/agent.rs`, `src/hooks.rs`).

Sources:
  - src/util.rs
  - src/hooks.rs
  - src/agent.rs
  - src/lib.rs
  - src/tools/mod.rs
  - src/default_context.rs
  - src/tools/control.rs
  - src/tools/mcp.rs
````

## Checked against the source

Both answers were checked against `swiftide-agents/src`.

- **Stop answer:** the stop reasons, the control tools (including `ApprovalRequired` leaving the
  tool call open), the retry limit (default 3, stops once a failing call exceeds it), the loop
  exits and the `stop` behaviour are correct. That includes the detail that hitting `limit` or
  `just_once` also ends in `NoNewMessages`, and that the final `stop` is a no-op when a tool
  already stopped the agent. One thing is flagged as an inference because the
  `DefaultContext::next_completion` chunk was not among the 20 retrieved: the common case, where
  `next_completion` returns `None` when `stop_on_assistant` is set (the default) and the last
  message is an assistant reply. The inference is correct.
- **Hooks answer:** the hook list, argument types, call order (including pending tool calls
  firing `BeforeTool`/`AfterTool` before `OnStart`, and the loop skipping `step` for unfinished
  tool calls), registration methods (`add_hook`, `hooks`) and error handling in `invoke_hooks!`
  all match the source. The code example is the model's own and was not compiled.
