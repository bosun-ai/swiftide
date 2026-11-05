---
name: swiftide
description: Write Rust code with Swiftide 0.33, a library for LLM agents, typed task graphs and streaming RAG (indexing and query) pipelines. Covers installing with the right feature flags, building agents with #[swiftide::tool] tools, hooks, structured stop payloads, human approval and MCP toolboxes, wiring Task graphs with fan-out, join and pause/resume, and indexing and querying with Qdrant, LanceDB, FastEmbed and OpenAI-compatible endpoints. Use when a Rust project depends on the swiftide crate (or swiftide-agents, swiftide-tasks, swiftide-indexing, swiftide-query), when the user asks to build an agent, tool-calling loop, task graph or RAG pipeline in Rust, or when fixing Swiftide compile errors such as missing feature flags.
license: MIT
---

# Swiftide

Swiftide is a Rust library for building LLM applications. It has three building blocks that share
one set of core traits (`SimplePrompt`, `ChatCompletion`, `EmbeddingModel`, `Tool`, ...):

| Building block | Use it for | Entry point |
| --- | --- | --- |
| Agents | A model calling tools in a loop until it stops | `swiftide::agents::Agent::builder()` |
| Tasks | A typed graph of steps (prompts, agents, functions) with fan-out, join and pause | `swiftide::tasks::Task::new()` |
| Pipelines | Streaming indexing (load, chunk, enrich, embed, store) and querying (transform, retrieve, answer) | `swiftide::indexing::Pipeline`, `swiftide::query::Pipeline` |

This skill targets **0.33**. Pre-1.0 minor releases break APIs; when the version in `Cargo.lock`
differs, check the CHANGELOG before trusting snippets from older blog posts or answers.

## Install

Default features enable only `macros`. Everything else is opt-in.

```toml
[dependencies]
swiftide = { version = "0.33", features = ["swiftide-agents", "openai"] }
tokio = { version = "1", features = ["full"] }
anyhow = "1"
```

Common feature flags (all on the `swiftide` crate):

- Orchestration: `swiftide-agents` (also enables `swiftide-tasks`), `swiftide-tasks`, `mcp`,
  `agents-contrib-file-tools`, `agents-contrib-planning`, `agents-contrib-context`,
  `agents-contrib-lazy-executor`
- LLMs: `openai`, `anthropic`, `gemini`, `groq`, `ollama`, `open-router`, `dashscope`, `aws-bedrock`
- Embeddings and reranking: `fastembed` (local dense, sparse and `Rerank`)
- Storage and caches: `qdrant`, `lancedb`, `pgvector`, `redis`, `redb`, `duckdb`
- Indexing: `tree-sitter` (code chunking), `scraping`, `kafka`, `fluvio`, `parquet`
- Other: `langfuse`, `metrics`, `json-schema`, `tiktoken`, `test-utils`

If you pass `default-features = false`, add `macros` back to use `#[swiftide::tool]` and
`#[derive(Tool)]`. Tools and structured stops also need `schemars` (with `derive`) and `serde`.

## Gotchas

1. **Feature flags are opt-in.** A missing `swiftide::agents`, `swiftide::integrations::openai` or
   `swiftide::integrations::qdrant` path means a missing feature, not a wrong import.
2. **No TLS setup is needed.** Swiftide's HTTP clients use rustls with a built-in crypto
   provider. Do not add `rustls` or call `install_default()` unless the app wants a specific
   provider; one installed before the first client is created takes precedence.
3. **Hooks return `Box::pin(async { ... })`.** Every agent hook is a closure that returns a pinned
   future of `Result<()>`; clone what you need before the `async move` block.
4. **MSRV is Rust 1.97, edition 2024.**
5. **OpenAI-compatible endpoints** (gateways, local servers, vLLM, LiteLLM) go through the
   `openai` integration: `OpenAI::builder().api_base(url).api_key(key)`; see
   `references/integrations.md`.
6. **Call `without_default_stop_tool()` before `.tools(...)`** when you supply your own
   `StopWithArgs`. `.tools(..)` merges in the default tools when it is called, and calling it
   again replaces the earlier list.
7. **MCP tools are named `<server>:<tool>`.** OpenAI rejects `:` in tool names; the
   release-notes-agent showcase wraps the toolbox to rename them (`references/agents.md`).
8. **`StopReason::NoNewMessages`** is reported both when the model answers without a tool call and
   when `.limit(n)` is reached. `limit` counts completions per `query`.
9. **Use the same embedding model for indexing and querying**, and set the store's
   `vector_size` to that model's dimensions.

## Agents

Tools are async functions; the macro derives the JSON schema from the arguments. Descriptions come
from the attribute or from the doc comment with an `# Arguments` list. From
`examples/release-notes-agent/src/tools.rs`:

```rust
/// Reads a file from the repository, e.g. to check a public API or the style of the changelog.
///
/// # Arguments
/// - `path`: File path relative to the repository root.
#[swiftide::tool]
pub async fn read_file(context: &dyn AgentContext, path: &str) -> Result<ToolOutput, ToolError> {
    let path: RepoPath = path.parse()?;
    let content = context
        .executor()
        .exec_cmd(&Command::read_file(path.as_path()))
        .await?;

    Ok(ToolOutput::text(clip(
        content.into_string_lossy(),
        MAX_FILE_LINES,
        "The file is longer than shown.",
    )))
}
```

Build the agent with a `Prompt` as system prompt, a custom executor, a structured `stop` tool and
an approval-gated write tool. From `examples/release-notes-agent/src/agent.rs`:

```rust
let mut builder = Agent::builder();
builder
    .llm(llm)
    .context(DefaultContext::from_executor(executor))
    .system_prompt(task.system_prompt(github_tools.is_some()))
    // Our own `stop` tool replaces the default one; call this before `tools`.
    .without_default_stop_tool()
    .tools([
        git_log_since(),
        git_diff(),
        read_file(),
        // Drafts that describe a change twice go back to the model before anyone reviews them.
        OneEntryPerChange::new(ApprovalRequired::new(WriteChangelog::new(&task.notes_file)))
            .into(),
        ReleaseNotes::stop_tool().into(),
    ])
    .limit(STEP_LIMIT);

if let Some(github_tools) = github_tools {
    builder.add_toolbox(github_tools);
}
```

The system prompt is a Tera template (`{{ var }}`, `{% if %}`, `{% for %}`):

```rust
let mut prompt = Prompt::from(SYSTEM_PROMPT)
    .with_context_value("since", self.since.as_str())
    .with_context_value("github_tools", github_tools && self.github.is_some());
if let Some(version) = &self.next_version {
    prompt.insert_context_value("next_version", version.as_str());
}
```

Key APIs: `agent.query(prompt)` (runs until stop), `agent.query_once(prompt)`, `agent.run()`
(resume), `agent.stop_reason()`, `agent.history()`, `agent.context()`. Stop reasons:
`StopReason::RequestedByTool(call, Option<Value>)`, `FeedbackRequired { tool_call, .. }`,
`NoNewMessages`, `AgentFailed(_)`, `ToolCallsOverLimit(_)`, `Error`.

Structured output: `StopWithArgs::with_parameters_schema(schema_for!(MyType))`, then read
`StopReason::RequestedByTool(_, Some(payload))` and deserialize. Human approval: wrap a tool in
`ApprovalRequired::new(tool)`, then `agent.context().feedback_received(&tool_call,
&ToolFeedback::approved())` and `agent.run()`. MCP: `McpToolbox::try_from_transport(transport)` or
`McpToolbox::from_running_service(service)`, then `.with_whitelist([...])` and `add_toolbox`.
Details and full snippets: `references/agents.md`.

## Tasks

A `Task<Input, Output>` is a typed graph. Register nodes, set the start, and register one
transition per node. From `examples/tasks_fanout.rs`:

```rust
let mut task = Task::new().with_default_concurrency_model(ConcurrencyModel::Parallel);

let start =
    task.register_node_fn(|input: &i32| -> std::result::Result<i32, NodeError> { Ok(*input) });
let increment = task
    .register_node_fn(|input: &i32| -> std::result::Result<i32, NodeError> { Ok(*input + 1) });
let double = task
    .register_node_fn(|input: &i32| -> std::result::Result<i32, NodeError> { Ok(*input * 2) });
let join = task.register_node_fn(
    |input: &JoinInput<i32>| -> std::result::Result<i32, NodeError> {
        Ok(input.iter().copied().sum::<i32>())
    },
);

task.starts_with(start);

task.register_transition(start, move |input| {
    Transition::fan_out(&increment, input)
        .and(&double, input)
        .join_with(join.join())
})?;

task.register_transition(increment, join.join())?;
task.register_transition(double, join.join())?;
task.register_transition(join, task.transitions_to_finish())?;
```

- Nodes: `register_node(impl TaskNode)`, `register_node_fn`, `register_node_async_fn`.
  `Arc<dyn SimplePrompt>` is a node out of the box (`Prompt` in, `String` out).
- Custom steps (for example an agent with a typed output) implement `TaskNode` with `Input`,
  `Output`, `Error` and `evaluate`.
- Transitions: `node_id.transitions_with(value)`, `Transition::next(&id, value)`,
  `Transition::fan_out(..).and(..).join_with(join.join())`, `Transition::pause()`,
  `Transition::error(..)`, `task.transitions_to_finish()`.
- Run: `task.run(input).await?` returns `TaskRunOutcome::Completed(out)` or `Paused`; resume with
  `task.resume()` and inspect paused nodes with `task.current_nodes::<MyNode>()`.

Details: `references/tasks.md`.

## Pipelines

Indexing streams `Node<T>` values (`TextNode = Node<String>`) through transformers. Steps run
concurrently. From `examples/index_codebase.rs`:

```rust
indexing::Pipeline::from_loader(FileLoader::new(".").with_extensions(&["rs"]))
    .filter_cached(Redis::try_from_url(redis_url, "swiftide-examples")?)
    .then(MetadataQACode::new(openai_client.clone()))
    .then_chunk(ChunkCode::try_for_language_and_chunk_size(
        "rust",
        10..2048,
    )?)
    .then_in_batch(Embed::new(openai_client.clone()).with_batch_size(10))
    .then_store_with(
        Qdrant::builder()
            .batch_size(50)
            .vector_size(1536)
            .collection_name("swiftide-examples")
            .build()?,
    )
    .run()
    .await?;
```

Querying is a typestate pipeline: a search strategy, query transformers, `then_retrieve`, optional
response transformers, `then_answer`. The query moves through `states::Pending`,
`states::Retrieved` and `states::Answered`. From `examples/query_pipeline.rs`:

```rust
let pipeline = query::Pipeline::default()
    .then_transform_query(query_transformers::GenerateSubquestions::from_client(
        openai_client.clone(),
    ))
    .then_transform_query(query_transformers::Embed::from_client(
        openai_client.clone(),
    ))
    .then_retrieve(qdrant.clone())
    .then_transform_response(response_transformers::Summary::from_client(
        openai_client.clone(),
    ))
    .then_answer(answers::Simple::from_client(openai_client.clone()));
```

`pipeline.query("...")` returns `Query<states::Answered>`; use `.answer()` and `.documents()`.
Details (custom loaders, chunkers, closures as transformers, hybrid search, reranking, Ragas):
`references/pipelines.md`.

## References

- `references/agents.md`: tools (`#[swiftide::tool]`, `#[derive(Tool)]`, manual `Tool`), hooks,
  streaming, structured stop, approval loops, MCP, executors, resuming from history.
- `references/tasks.md`: `TaskNode` implementations, agents as nodes, fan-out over a list, join,
  pause and resume for approval.
- `references/pipelines.md`: indexing and query pipelines, custom `Loader`, `ChunkerTransformer`,
  metadata prompts, caching, hybrid search, reranking, evaluation.
- `references/integrations.md`: OpenAI-compatible endpoints, backoff, other providers, FastEmbed,
  Qdrant, LanceDB, Redb, pgvector, Langfuse.

Runnable sources live in the Swiftide repository under `examples/`, including four complete
showcase crates: `examples/ask-your-codebase` (local code RAG), `examples/release-notes-agent`
(agent with tools, MCP and approval), `examples/workspace-architect` (task graph with fan-out and
pause) and `examples/docs-qa` (hybrid search with evaluation). API docs: https://docs.rs/swiftide.

## Before you finish

- `cargo check` with the exact features you added; a missing feature shows up as an unresolved
  import.
- Tools return `Result<ToolOutput, ToolError>`; use `ToolOutput::fail(..)` for errors the model
  should see and recover from.
