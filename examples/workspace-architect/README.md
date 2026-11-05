# workspace-architect

A Swiftide showcase that drafts an `ARCHITECTURE.md` for any Cargo workspace.

It is built as a typed task graph with `swiftide::tasks`:

```text
discover ──fan-out──▶ analyse × N (parallel) ──▶ join (builds prompt) ──▶ render ──▶ review ──▶ write
                                                                                     │  ▲
                                                                            pause ◀──┘  └── resume
```

1. **discover** runs `cargo metadata` and lists the workspace member crates, foundations first.
2. **analyse** fans out to one branch per crate, all running in parallel. Each branch builds an
   agent scoped to its crate's directory. The agent has read-only `list_files`, `read_file` and
   `grep` tools and must finish by calling `stop` with a typed `CrateReport`. The node checks the
   report against `cargo metadata` (declared dependencies win over the model's list) and hands a
   `CrateSummary` to the join.
3. **join** collects every `CrateSummary` (`JoinInput<CrateSummary>`) and builds a Tera prompt.
   The Mermaid dependency graph in the prompt comes from `cargo metadata`, not from the model.
4. **render** is a plain `Arc<dyn SimplePrompt>` node that writes the Markdown document.
5. **review** pauses the task. The CLI shows the draft and asks for approval, then calls
   `task.resume()`. Pass `--yes` to skip the pause.
6. **write** saves the file.

Each node prints a progress line. The join line compares total agent time with wall-clock time,
which shows the speed-up from running the branches in parallel.

## Prerequisites

- An OpenAI-compatible chat API that supports tool calls: `OPENAI_API_KEY` (required) and,
  for anything other than `OpenAI` itself, `OPENAI_BASE_URL`. A `.env` file in the working
  directory is loaded too; parent directories are not searched. Empty values count as unset.
  The CLI checks for the key before it starts any agent.
- `cargo` on the `PATH` (for `cargo metadata`), plus `find` and `grep`, which the agent tools use.

No Docker or database is needed.

## Running

To document the Swiftide workspace itself, run this from the repository root:

```bash
export OPENAI_API_KEY=sk-...
cargo run -p workspace-architect
```

Through any other OpenAI-compatible endpoint, for example a local gateway:

```bash
export OPENAI_BASE_URL=http://127.0.0.1:8080/v1
export OPENAI_API_KEY=...
export OPENAI_MODEL=claude-sonnet-5-5
cargo run -p workspace-architect
```

To document another workspace without the approval pause:

```bash
cargo run -p workspace-architect -- --workspace ../other-repo --yes
```

| Variable | Default | Description |
| --- | --- | --- |
| `OPENAI_API_KEY` | (required) | API key for the endpoint |
| `OPENAI_BASE_URL` | `https://api.openai.com/v1` | Any OpenAI-compatible API |
| `OPENAI_MODEL` | `gpt-5.4` | Same as `--model` |
| `WORKSPACE_ARCHITECT_LOG` | off | Tracing filter in `RUST_LOG` syntax; output goes to stderr |

| Flag | Default | Description |
| --- | --- | --- |
| `--workspace <PATH>` | `.` | Workspace to document. Any directory inside it works; the root is found with `cargo metadata`. |
| `--output <PATH>` | `ARCHITECTURE.md` | Output file. Relative paths are resolved against the workspace root. |
| `--yes` | off | Write the draft without pausing for approval |
| `--all-members` | off | Also document unpublished members (examples, benchmarks, `publish = false`) |
| `--model <MODEL>` | `gpt-5.4` | Chat model for the analysts and the writer (env: `OPENAI_MODEL`) |
| `--responses-api` | off | Call the model through the Responses API instead of Chat Completions |
| `--verbose` | off | Print every tool call and the raw summary each agent returns |

Swiftide's own tracing is off unless `WORKSPACE_ARCHITECT_LOG` is set, e.g.
`WORKSPACE_ARCHITECT_LOG=swiftide=debug`, and it goes to stderr. The CLI ignores `RUST_LOG` on
purpose: a global cargo config often sets it in its `[env]` table, `cargo run` passes it on, and
the debug output would bury the draft you are asked to approve.

Every completion may use up to 16,384 output tokens, so the whole document fits in one answer
even behind gateways that cap an unset limit at 4,096 tokens.

When an agent answers without calling `stop`, or passes arguments that do not match
`CrateReport` (some models nest them under an `arguments` key), the analyst tells it what was
wrong and asks again, up to twice. The progress output shows these as `retry` lines.

Each agent query may use up to 30 completions. Swiftide resets that budget on every query, so
each correction gets a fresh one, and a single crate can use up to 90 completions in the worst
case. In the runs in OUTPUT.md an analysis took 4 to 15 tool calls.

The tools accept only relative paths without `..`. Symlinks inside a crate are followed, so this
is a guard rail for a read-only demo, not a sandbox.

See [OUTPUT.md](OUTPUT.md) for real runs on the Swiftide repository. Its approved-run block
contains the complete document that run wrote.

`cargo test -p workspace-architect` runs the whole graph offline on this workspace with a
scripted model, including the pause, approval and `resume()`.

## Anchors

The documentation excerpts these regions with `::snippet{src="…" anchor="name"}`:

| Anchor | File | Shows |
| --- | --- | --- |
| `graph` | `src/graph.rs` | The whole task definition |
| `nodes` | `src/graph.rs` | Registering closure nodes, a custom `TaskNode`, and `Arc<dyn SimplePrompt>` as nodes |
| `fan_out` | `src/graph.rs` | A dynamic fan-out: one branch per crate on the same node, run with `ConcurrencyModel::Parallel` |
| `join` | `src/graph.rs` | Routing each branch to the join, and the join to the render node |
| `pause` | `src/graph.rs` | Branching on the review result: continue to `write` or `Transition::pause()` |
| `resume` | `src/main.rs` | The run, pause, approve and `resume()` loop in the CLI |
| `approval` | `src/approval.rs` | The approval gate node the CLI reaches through `task.current_nodes()` |
| `analyst` | `src/analyst.rs` | The agent-backed node struct |
| `agent` | `src/analyst.rs` | Building a per-crate agent: `LocalExecutor` scoped to the crate, tools, custom stop tool |
| `hooks` | `src/analyst.rs` | A `before_tool` hook that reports each tool call |
| `task_node` | `src/analyst.rs` | `TaskNode::evaluate`: run the agent, read the structured `stop` arguments (asking again, at most `MAX_CORRECTIONS` times, if they are missing or malformed), reconcile them |
| `stop_schema` | `src/summary.rs` | `CrateReport` and `StopWithArgs::with_parameters_schema` |
| `tools` | `src/tools.rs` | `#[swiftide::tool]` tools that run commands through the agent's executor |
| `render` | `src/render.rs` | The Tera template and prompt built from `JoinInput<CrateSummary>` |

`cargo test -p workspace-architect` also covers the reconciliation, including a model that echoes
`extras (optional)` from the brief as the crate name.
