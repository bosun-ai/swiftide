# release-notes-agent

An agent that writes the release notes for your next release. It reads the git history since the
last tag, looks at the diffs that matter, streams a draft to your terminal, and writes it to
`RELEASE_NOTES.md` only after you approve.

```text
$ release-notes-agent --since v0.33.0 --version 0.34.0 --model claude-sonnet-5-5
Drafting release notes for the 7 commits since v0.33.0 with claude-sonnet-5-5 at http://127.0.0.1:8080/v1 (chat completions)
  → git_log_since v0.33.0
  git_log_since ✓ 346 lines
  → git_diff swiftide-integrations/Cargo.toml 3b1ad22e^..3b1ad22e
  ...
## 0.34.0
...                                   <- streamed while the model writes it
── proposed: 32 lines into a new file, RELEASE_NOTES.md
Write these release notes? [y/N] y
  → write_changelog <32 lines>
  write_changelog ✓ wrote RELEASE_NOTES.md
```

See [OUTPUT.md](OUTPUT.md) for the complete run and the notes it wrote.

It shows how to:

- define tools with `#[swiftide::tool]` (taking descriptions from doc comments) and with
  `#[derive(Tool)]` for a tool with state;
- run every tool through a `LocalExecutor` whose working directory is the repository, and parse
  model-provided arguments into value types (`Revision`, `RepoPath`) so they cannot leave the
  repository or inject shell;
- build the system prompt with `Prompt` (a Tera template);
- print tool calls with `before_tool`/`after_tool` hooks, and stream the draft with `on_stream`,
  decoding the streamed tool-call arguments as they arrive;
- put a human in the loop with `ApprovalRequired`: the agent stops with
  `StopReason::FeedbackRequired`, you approve or ask for changes, and the agent resumes;
- check a draft in code before a human sees it: a tool wrapping `ApprovalRequired` sends drafts
  that describe one pull request in several entries back to the model;
- end with structured output: a `StopWithArgs` tool whose JSON schema is `ReleaseNotes`, read back
  from `StopReason::RequestedByTool`;
- add tools from an MCP server (`McpToolbox`), and trace everything to Langfuse.

## Prerequisites

- Rust (the workspace toolchain) and a git repository with at least one tag.
- `OPENAI_API_KEY`, and optionally `OPENAI_BASE_URL` (default `https://api.openai.com/v1`) to use
  any `OpenAI`-compatible API instead, such as a local gateway. Empty values count as unset.
- For `--mcp`: Node.js (`npx`) and `GITHUB_PERSONAL_ACCESS_TOKEN` (or `GITHUB_TOKEN`). The agent
  gets two read-only tools, `get_pull_request` and `get_issue`, to read the pull requests behind
  commits. Without `npx`, a token, or an `origin` remote on GitHub, it says so and continues
  without them.
- For `--trace`: `LANGFUSE_PUBLIC_KEY` and `LANGFUSE_SECRET_KEY`, and optionally `LANGFUSE_URL`
  (default `http://localhost:3000`). A local Langfuse runs in Docker:

  ```sh
  docker network create langfuse
  docker run -d --name langfuse-db --network langfuse \
    -e POSTGRES_USER=postgres -e POSTGRES_PASSWORD=postgres postgres:16
  docker run -d --name langfuse --network langfuse -p 3000:3000 \
    -e DATABASE_URL=postgresql://postgres:postgres@langfuse-db:5432/postgres \
    -e NEXTAUTH_SECRET=secret -e SALT=salt -e NEXTAUTH_URL=http://localhost:3000 \
    -e LANGFUSE_INIT_ORG_ID=local -e LANGFUSE_INIT_PROJECT_ID=release-notes \
    -e LANGFUSE_INIT_PROJECT_PUBLIC_KEY="$LANGFUSE_PUBLIC_KEY" \
    -e LANGFUSE_INIT_PROJECT_SECRET_KEY="$LANGFUSE_SECRET_KEY" \
    langfuse/langfuse:2
  ```

Environment variables are also read from `./.env`, or from the file given with `--env-file`.

## Running

From the repository you want release notes for:

```sh
cargo run -p release-notes-agent -- --version 0.34.0
```

with any `OpenAI`-compatible API, here a local gateway:

```sh
OPENAI_BASE_URL=http://127.0.0.1:8080/v1 OPENAI_MODEL=claude-sonnet-5-5 \
  cargo run -p release-notes-agent -- --version 0.34.0
```

or point it at another repository:

```sh
cargo run -p release-notes-agent -- --repo ../my-crate --since v1.2.0 --mcp --trace
```

When the draft is done you are asked `Write these release notes? [y/N]`. Answer `y` to write them.
Answer anything else and you are asked what should change: the agent gets your feedback, revises,
and asks again. An empty answer stops without writing anything.

| Flag | Default | |
| --- | --- | --- |
| `--repo <path>` | `.` | The git repository. |
| `--since <tag>` | latest tag | The previous release. |
| `--version <next>` | proposed by the agent | The version being released. |
| `--output <file>` | `RELEASE_NOTES.md` | File the notes are prepended to, relative to the repository root (also when `--repo` is a subdirectory). |
| `--mcp` | off | Read pull requests through the GitHub MCP server. |
| `--trace` | off | Send traces to Langfuse. |
| `--model <name>` | `gpt-5.4` | Model served by `OPENAI_BASE_URL`, also `OPENAI_MODEL`. |
| `--responses-api` | off | Use the `OpenAI` Responses API instead of chat completions. |
| `--env-file <path>` | `./.env` | Load environment variables from this file. |

Logs go to stderr at `warn`; set `RUST_LOG` to change that.

## Layout

| File | Responsibility |
| --- | --- |
| `src/main.rs` | CLI: flags, wiring, output. |
| `src/agent.rs` | The release task, system prompt and agent. |
| `src/llm.rs` | The `OpenAI`-compatible endpoint, from `OPENAI_BASE_URL` and `OPENAI_API_KEY`. |
| `src/system_prompt.md` | The instructions, as a Tera template. |
| `src/tools.rs` | The agent's tools, and the duplicate-entry check around `write_changelog`. |
| `src/git.rs` | Git queries through the executor, and the value types that guard them. |
| `src/session.rs` | The approval loop, independent of the terminal. |
| `src/terminal.rs` | Asking for approval on stdin. |
| `src/progress.rs` | Hooks that print tool calls and stream the draft. |
| `src/draft.rs` | Decoding a streamed, JSON-encoded tool argument. |
| `src/notes.rs` | `ReleaseNotes`, the structured result. |
| `src/mcp.rs` | The GitHub MCP toolbox. |
| `src/telemetry.rs` | Logging and Langfuse. |

## Anchors

The documentation includes these regions of the source:

| Anchor | File | Shows |
| --- | --- | --- |
| `executor` | `src/main.rs` | A `LocalExecutor` with the repository as working directory. |
| `tools` | `src/tools.rs` | `git_log_since`, `git_diff`, `read_file` with `#[swiftide::tool]`. |
| `write_changelog` | `src/tools.rs` | A stateful tool with `#[derive(Tool)]`, writing through the executor without clobbering the file when a read fails. |
| `prompt` | `src/agent.rs` | The system prompt as a `Prompt` with template values. |
| `agent` | `src/agent.rs` | Building the agent: context, tools, a draft check around approval, custom stop tool. |
| `hooks` | `src/progress.rs` | `before_tool` and `after_tool`. |
| `stream` | `src/progress.rs` | `on_stream`, showing a tool argument while it streams. |
| `stop_schema` | `src/notes.rs` | `ReleaseNotes` and its `StopWithArgs` tool. |
| `render` | `src/notes.rs` | Printing the structured result. |
| `approval` | `src/session.rs` | Handling `FeedbackRequired`, approving or refusing, resuming, and only reporting success when the approved write succeeded. |
| `mcp` | `src/mcp.rs` | Starting an MCP server and adding a whitelisted `McpToolbox`. |
| `langfuse` | `src/telemetry.rs` | The Langfuse tracing layer. |

## Notes

- The notes are a draft. The prompt asks the model to classify commits by their diff and the
  tool rejects duplicate entries, but read them against `git show` before you approve.
- What ran live, through an `OpenAI`-compatible gateway with `claude-sonnet-5-5`: the approve path
  (OUTPUT.md); declining with an empty answer; the duplicate-entry check, which sent a draft that
  listed #1215 under both Breaking changes and Fixes back to the model; and `--mcp`, whose GitHub
  server started and registered its tools (the model did not need them, since the commit messages
  carry the pull request descriptions). The revise round, stopping without a write, the nudge
  after an empty turn, giving up after a second one and the step-limit check are covered by
  scripted tests (`cargo test -p release-notes-agent`).
- `--trace` was not run against a live Langfuse for this build; no server was reachable.
- `--responses-api` is off by default. Against the gateway used for OUTPUT.md it fails: the
  gateway streams Responses events with `"output": null`, which `async-openai` cannot parse. Use it
  with `OpenAI` itself.
- Some models end a turn without calling a tool right after reading tool results. The session
  then asks once to continue (`nudge_once` in `src/session.rs`), and gives up if it happens again.
  Swiftide reports reaching the agent's step limit (`STEP_LIMIT`, 40) with the same
  `StopReason::NoNewMessages`; the session tells the two apart from the history and does not
  nudge at the limit.
- `ScopedExecutor`/`LocalExecutor` set the working directory; they do not sandbox. Absolute paths
  and `..` would still escape, which is why tool arguments are parsed into `RepoPath` first.
- Swiftide registers MCP tools as `<server>:<tool>`, which `OpenAI` rejects, so `src/mcp.rs`
  renames them (`github-mcp-server_get_pull_request`).
- The GitHub MCP server has no tool to create releases; this example only reads pull requests.
