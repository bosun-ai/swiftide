# docs-qa: captured output

A real run on 2026-11-05 over the Swiftide repository's markdown docs, with Qdrant in Docker.
Stdout is copied verbatim, except where `...` marks a trimmed passage; stderr was empty for
every command.

| | |
| --- | --- |
| Endpoint | OpenAI-compatible gateway (Bifrost at `http://127.0.0.1:8080/v1`, routing to AWS Bedrock), chat completions API |
| Chunk titles and keywords | `claude-haiku-5-5` |
| Answers | `claude-sonnet-5-5` |
| Judge | `claude-sonnet-5-5` |
| Embeddings and reranking | local FastEmbed: BGE small en v1.5 (dense), SPLADE++ (sparse), BGE reranker base |

## Setup

```sh
docker run -d --name docs-qa-qdrant -p 6333:6333 -p 6334:6334 qdrant/qdrant
cargo build -p docs-qa

export OPENAI_BASE_URL=http://127.0.0.1:8080/v1
export OPENAI_API_KEY=sk-bf-...   # the gateway's virtual key
```

All commands ran from the repository root, with a debug build.

## Index

```sh
./target/debug/docs-qa index --recreate --metadata-model claude-haiku-5-5 \
  README.md CONTRIBUTING.md AGENTS.md swiftide-agents-contrib/README.md
```

```text
Indexed 43 chunks into `docs-qa` in 71.7s
```

Four pages became 43 chunks. Each chunk got an LLM-written title and keywords (86 calls to
Haiku), then a dense and a sparse embedding. The chunk titles show up as the section names in
the sources below.

## Ask

```sh
./target/debug/docs-qa --model claude-sonnet-5-5 ask "How do I give an agent tools to read and edit files?"
```

````text
Enable the `agents-contrib-file-tools` feature on `swiftide`, then register `ReadFile`, `ExactReplace`, and `ApplyPatch` on your agent with `add_tool`.

**Enable the feature**

```sh
cargo add swiftide --features agents-contrib-file-tools
```

If you depend on the contrib crate directly, use the `file-tools` feature on `swiftide-agents-contrib`.

**Register the tools**

```rust
use swiftide_agents::Agent;
use swiftide_agents_contrib::tools::{ApplyPatch, ExactReplace, ReadFile};

Agent::builder()
    .add_tool(ReadFile::default())
    .add_tool(ExactReplace)
    .add_tool(ApplyPatch);
// Configure your model and executor, then build and run the agent.
```

Through the main crate, import from `swiftide::agents_contrib` (for example, `swiftide::agents_contrib::tools::ReadFile`).

**What you get**

- `read_file`: takes `file_name` and optional one-based `offset` and `limit`. It defaults to 2,000 lines, truncates long lines at 2,000 characters, and refuses windows over 100,000 characters.
- `read_file_with_line_numbers`: use `ReadFile::default().with_line_numbers()` to get the same reader with `N|` line numbers.
- `edit` (`ExactReplace`): takes `file_path`, `old_string`, `new_string`, and optional `replace_all`. It replaces a unique exact match, or all matches when requested.
- `ApplyPatch`: edits files via patches.

**Requirement:** the executor must provide a POSIX-like shell with `cat`, `mkdir`, and `rm` commands.

Sources:
  [1] Swiftide › Agent Tools and Hooks: Enabling Contrib File, Planning, and Context Features  (README.md)
  [2] swiftide-agents-contrib › read_file tool: arguments, defaults, and offset/limit behavior  (swiftide-agents-contrib/README.md)
  [3] swiftide-agents-contrib › File Tools: Line-Numbered Reads and Exact-Match Edits  (swiftide-agents-contrib/README.md)
  [4] swiftide-agents-contrib › Adding file and plan tools with Agent::builder().add_tool  (swiftide-agents-contrib/README.md)
  [5] swiftide-agents-contrib › ReadFile tool: read_file and with_line_numbers variants  (swiftide-agents-contrib/README.md)
  ...
  [9] Swiftide › AgentContext conversation history and ToolExecutor access  (README.md)
  [10] Swiftide › Swiftide capabilities for agents, typed task graphs, pipelines, and tracing  (README.md)
````

```sh
./target/debug/docs-qa --model claude-sonnet-5-5 ask "Which LLM providers does Swiftide support, and how do I enable one?"
```

````text
Swiftide supports OpenAI and Azure OpenAI, Anthropic, Gemini, OpenRouter, AWS Bedrock Converse API, Groq, Ollama, and Dashscope. Integrations are feature-gated, so you enable a provider by adding its Cargo feature to `swiftide`.

## Enabling a provider

The documentation shows the OpenAI example:

```sh
cargo add swiftide --features swiftide-agents,openai
cargo add anyhow
cargo add tokio --features macros,rt-multi-thread
```

Then set the API key the integration expects:

```sh
export OPENAI_API_KEY=...
```

Build the client and pass it to an agent:

```rust
let openai = swiftide::integrations::openai::OpenAI::builder()
    .default_prompt_model("gpt-4o-mini")
    .build()?;

agents::Agent::builder()
    .llm(&openai)
    // ...
```

## Other providers

The documentation lists the other providers but doesn't give their exact feature flag names or builder APIs. Presumably they follow the same pattern as `openai`, but I can't confirm that from the documentation. Check the [API docs](https://docs.rs/swiftide/latest/swiftide/) or the [examples](https://github.com/bosun-ai/swiftide/tree/master/examples) for the exact feature name of your provider.

Sources:
  [1] Swiftide › Swiftide capabilities for agents, typed task graphs, pipelines, and tracing  (README.md)
  [2] Swiftide › Swiftide: Rust framework for LLM agents, task graphs, and RAG  (README.md)
  [3] Swiftide › Swiftide Integrations Overview by Feature-Gated Category  (README.md)
  ...
  [9] Swiftide › Swiftide agent tool defined with the swiftide::tool macro  (README.md)
  [10] Swiftide › Installing Swiftide and Setting Up OpenAI Integration  (README.md)
````

The docs only show the feature flag for `openai`, and the answer says so instead of guessing
the flag names for the other providers. That flag comes from the Quick Start chunk, which the
reranker places tenth here, so it only reaches the answer because ten chunks are kept.

## Evaluate

```sh
./target/debug/docs-qa --model claude-sonnet-5-5 eval --judge-model claude-sonnet-5-5 \
  --out examples/docs-qa/ragas.json
```

```text
#   question                                              ctx faithful   answer  context
                                                                         recall   recall
1   Which crate features and environment variable do I …   10     1.00     1.00     1.00
2   What does the agents-contrib-context feature provid…   10     1.00     1.00     1.00
3   What are the default limits of the read_file tool?     10     1.00     1.00     1.00
4   Which storage and retrieval backends does Swiftide …   10     1.00     1.00     1.00
5   What should I run before opening a pull request?       10     1.00     1.00     1.00
6   What is a task in Swiftide?                            10     1.00     1.00     1.00
7   When does ContextCompression summarize the conversa…   10     1.00     1.00     1.00
8   Which commit message convention does the project fo…   10     1.00     1.00     1.00
9   How do I add a custom tool to an agent with the too…   10     1.00     1.00     1.00
    mean                                                          1.00     1.00     1.00

RAGAS dataset written to examples/docs-qa/ragas.json
```

The RAGAS dataset is in [`ragas.json`](ragas.json): nine rows, in the order of
`questions.yaml`, with the question, the answer, the ground truth and the 20 retrieved
candidates per question.

Every question scores 1.00 here, but two of them did not before retrieval was tuned, and that
is what the eval is for:

- **What is a task in Swiftide?** The reference answer lists what tasks support (sequential or
  parallel branches, pause, resume and reset), and that list sits in its own README chunk. When
  the reranker scored only chunk text, that chunk ranked 14th and was dropped; in the
  2026-11-04 run the answer missed sequential execution and reset (answer recall 0.71, context
  recall 0.57). Reranking on
  page, chunk title and text moved it into the kept chunks.
- **How do I add a custom tool to an agent with the tool macro?** The README chunk with the
  `#[swiftide::tool]` example is almost all code. Its title used to describe the example tool
  (for example "Swiftide Agent Tool Example Using OpenAI and explain_concept"), the reranker
  dropped it (it came 11th of the 20 candidates, with eight kept), and the answer said the docs
  did not show the macro's syntax: answer recall and context recall were 0.00 in an earlier run
  of this eval. The title prompt now asks for code-only chunks to be named after the API they
  demonstrate. This run titled it "Swiftide agent tool defined with the swiftide::tool macro",
  and the answer shows the attribute, the function signature and `.tools([explain_concept()])`.

The scores come from one LLM judge over a small, nine-question set. Treat them as a regression
signal for this corpus, not as a benchmark.
