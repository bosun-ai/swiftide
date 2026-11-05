# Ask your codebase

A small CLI that indexes a codebase on your machine and answers questions about it, citing the
files it used.

```text
$ ask-your-codebase index swiftide-agents
$ ask-your-codebase ask --path swiftide-agents "How does an agent decide to stop?"
```

Everything except the LLM runs locally: tree-sitter chunks the code, FastEmbed embeds it on the
CPU, and the vectors live in an embedded LanceDB table. No Docker or database server is needed.

## How it works

**`index <path>`** runs a Swiftide indexing pipeline:

1. `FileLoader` loads the source files under `<path>` (Rust by default, see `--lang`), honouring
   `.gitignore`. Test files (`tests/` directories, `tests.rs`, `test_utils.rs`, `test_helpers.rs`)
   are skipped.
2. Unit tests (`#[cfg(test)] mod tests`, `#[test] fn`), with their doc comments and attributes,
   are cut out of Rust files with tree-sitter, so no chunk mixes implementation and tests.
3. `filter_cached` with a `Redb` node cache skips files that were indexed before and have not
   changed, so re-running `index` only processes what you edited.
4. A closure stores the file path, relative to `<path>`, as metadata.
5. `ChunkCode` (tree-sitter) splits each file along syntax boundaries into chunks of 10 to 4096
   bytes, so most functions stay in one piece. A small `ChunkByLanguage` chunker picks the grammar
   from the file extension.
6. `MetadataQACode` asks the LLM, with a custom prompt, which questions a developer could ask that
   each chunk answers, focused on behaviour ("When does the agent stop?"). The Q&A is stored as
   metadata and embedded along with the code, which helps a question find the right code.
7. `Embed` embeds chunk and metadata in batches with FastEmbed (`jina-embeddings-v2-base-code`,
   local, no key).
8. The chunks, their vectors and their paths are stored in LanceDB.

**`ask "<question>"`** runs a Swiftide query pipeline:

1. `GenerateSubquestions` widens the question into follow-up questions, phrased like the
   questions generated while indexing.
2. `Embed` embeds them with the same local model.
3. LanceDB returns the `--top-k` most similar chunks.
4. `answers::Simple` answers the original question from those chunks. A document template
   renders each chunk under its file path, so the answer can cite files.

The answer is followed by the distinct source files of the retrieved chunks.

The index lives next to the code, in `<path>/.ask-your-codebase/` (`cache.redb` and `lancedb/`).
Delete that directory to start over, for example after changing the embedding model.

## Prerequisites

- Rust (see the workspace `rust-toolchain.toml`).
- An OpenAI-compatible chat API, configured through the environment:

  | Variable | Required | Default | Description |
  | --- | --- | --- | --- |
  | `OPENAI_API_KEY` | yes | | API key. |
  | `OPENAI_BASE_URL` | no | `https://api.openai.com/v1` | Base URL of any OpenAI-compatible API, e.g. a local gateway. |
  | `OPENAI_MODEL` | no | `gpt-5.4-mini` | Model, same as `--model`. |

  Empty variables count as unset.

- Disk space and a network connection for the embedding model: FastEmbed downloads
  `jina-embeddings-v2-base-code` (about 600MB) into `.fastembed_cache/` in the current directory on
  first use.

## Running

From the repository root:

```bash
export OPENAI_API_KEY=sk-...

# Index a crate. Re-running only processes changed files.
cargo run -p ask-your-codebase -- index swiftide-agents

# Ask questions about it.
cargo run -p ask-your-codebase -- ask --path swiftide-agents "How does an agent decide to stop?"
cargo run -p ask-your-codebase -- ask --path swiftide-agents "What hooks can I register and when do they run?"
```

`index` and `ask` are separate runs, so they can use different models: a small, fast model for
the many per-chunk prompts while indexing, and a stronger one for answers. `OUTPUT.md` was made
that way, through an OpenAI-compatible gateway:

```bash
export OPENAI_BASE_URL=http://127.0.0.1:8080/v1
cargo run -p ask-your-codebase -- index swiftide-agents --model claude-haiku-5-5
cargo run -p ask-your-codebase -- ask --path swiftide-agents --model claude-sonnet-5-5 "How does an agent decide to stop?"
```

Use `RUST_LOG=info` (or `debug`) to see what the pipelines do. By default only errors are logged,
and the error of each retried LLM attempt is hidden: when retries run out, the command exits
with one error that names the endpoint and model it used, for example:

```text
Error: Indexing failed, using the LLM endpoint http://127.0.0.1:9/v1 (OPENAI_BASE_URL), model gpt-5.4-mini

Caused by:
    Transient error: error sending request for url (http://127.0.0.1:9/v1/chat/completions)
```

## Flags

| Flag | Applies to | Default | Description |
| --- | --- | --- | --- |
| `--model <name>` | both | `gpt-5.4-mini` | Model for chunk metadata (`index`) or answers (`ask`). Also `OPENAI_MODEL`. |
| `--responses-api` | both | off | Send prompts to the OpenAI Responses API instead of Chat Completions. |
| `--lang <languages>` | `index` | `rust` | Comma-separated tree-sitter languages, e.g. `rust,python,typescript`. Files are selected by extension. |
| `--concurrency <n>` | `index` | `16` | Prompts sent at once while indexing. Lower it for providers with tight rate limits. |
| `--path <dir>` | `ask` | `.` | Root of the codebase, as given to `index`. |
| `--top-k <n>` | `ask` | `20` | Number of chunks to retrieve. |

## Anchors

The documentation excerpts these regions (`// ANCHOR: name` ... `// ANCHOR_END: name`):

| Anchor | File | Shows |
| --- | --- | --- |
| `pipeline` | `src/index.rs` | The full indexing pipeline: load, skip tests, `filter_cached`, chunk, Q&A metadata, embed, store. |
| `questions_prompt` | `src/index.rs` | A custom `MetadataQACode` prompt that generates behaviour questions per chunk. |
| `path_metadata` | `src/index.rs` | A closure transformer that stores the relative file path as metadata. |
| `skip_tests` | `src/test_code.rs` | A `Pipeline::filter` predicate that drops test files, and a transformer that cuts unit tests out of Rust files. |
| `chunker` | `src/languages.rs` | A custom `ChunkerTransformer` that picks a tree-sitter `ChunkCode` per file extension. |
| `storage` | `src/codebase.rs` | Configuring the `Redb` node cache and the embedded `LanceDB` store, including a metadata column for the path. |
| `embedding` | `src/embedding.rs` | Picking a code embedding model for local FastEmbed embeddings. |
| `llm` | `src/llm.rs` | Pointing swiftide's `OpenAI` client at any OpenAI-compatible endpoint, wrapped in `LanguageModelWithBackOff` with a retry budget. |
| `query_pipeline` | `src/query.rs` | The query pipeline: a top-k similarity search strategy, subquestions, embed, retrieve, answer. |
| `prompts` | `src/query.rs` | Custom subquestion and answer prompts, and a document template that keeps file paths so answers can cite them. |
| `sources` | `src/query.rs` | Reading the distinct source paths from the retrieved documents. |

## Limitations

- When a file changes, its new chunks are added, but chunks of the old version stay in LanceDB
  until you delete `.ask-your-codebase/`.
- Test detection follows Rust conventions (`tests/` directories, `tests`, `test_utils` and
  `test_helpers` modules, and test attributes). Other languages are indexed with their tests.
- Retrieval is a single vector search. An answer only sees the `--top-k` closest chunks and says
  when something it needs is not among them; raise `--top-k` for broad questions.
- Rate limits, server errors and connection errors are retried with exponential backoff for up
  to 60 seconds per prompt during `index` and 30 seconds during `ask`. A wrong key fails at once;
  an unreachable or broken endpoint fails once that window has passed.
- A file that produces no chunks (for example one that only contains tests) is never marked as
  cached, so it is loaded again on every run. No LLM calls are made for it.
