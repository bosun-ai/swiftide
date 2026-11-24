# docs-qa

Ask questions about a set of markdown docs, and measure how good the answers are.

`docs-qa` is a small Swiftide RAG application with three commands:

- `index` loads markdown files (or an `llms-full.txt`) with a custom loader, chunks them, lets an
  LLM write a title and keywords per chunk, embeds every chunk twice (dense and sparse, locally
  with FastEmbed) and stores both vectors in Qdrant.
- `ask` runs a hybrid search (dense + sparse, fused by Qdrant), reranks the candidates with a
  local cross-encoder, and answers with an LLM. It prints the answer and the pages and sections
  it was based on.
- `eval` runs a fixed question set through the same pipeline with Swiftide's `Ragas`
  evaluator, writes the RAGAS dataset (`ragas.json`) and prints a table with claim-based
  faithfulness, answer recall and context recall scores from an LLM judge.

## Prerequisites

- Docker, for Qdrant. Swiftide talks to Qdrant over gRPC, so expose port 6334 as well:

  ```sh
  docker run -d --name docs-qa-qdrant -p 6333:6333 -p 6334:6334 qdrant/qdrant
  ```

  Set `QDRANT_URL` (and `QDRANT_API_KEY`) to use another instance.
- An OpenAI-compatible chat API. Embeddings never leave your machine, so only chat
  completions (with JSON-schema structured output, for the judge) are needed:

  | Variable | Required | Default | Meaning |
  | --- | --- | --- | --- |
  | `OPENAI_API_KEY` | yes | | API key, sent as a Bearer token |
  | `OPENAI_BASE_URL` | no | `https://api.openai.com/v1` | Base URL of the API: `OpenAI`, a gateway, or a local server |
  | `OPENAI_MODEL` | no | `gpt-5.4-mini` | Same as `--model`; also the default for `--metadata-model` and `--judge-model` |
- Disk space for the FastEmbed models (dense BGE small, sparse SPLADE and the BGE reranker,
  about 2.3 GB). They are downloaded on first use into `.fastembed_cache` in the working
  directory; set `FASTEMBED_CACHE_DIR` to share one cache between directories.
- Optional, for `--langfuse`: `LANGFUSE_PUBLIC_KEY` and `LANGFUSE_SECRET_KEY` (or the
  `LANGFUSE_INIT_PROJECT_PUBLIC_KEY` and `LANGFUSE_INIT_PROJECT_SECRET_KEY` that Langfuse's own
  docker-compose sets), and `LANGFUSE_URL` (defaults to `http://localhost:3000`).

Environment variables can also live in a `.env` file in the working directory.

## Running it

From the repository root:

```sh
export OPENAI_API_KEY=sk-...

# Index the repository docs
cargo run -p docs-qa -- index --recreate \
  README.md CONTRIBUTING.md AGENTS.md swiftide-agents-contrib/README.md

# Ask a question
cargo run -p docs-qa -- ask "How do I give an agent tools to read and edit files?"

# Evaluate the bundled question set and export a RAGAS dataset next to this README
cargo run -p docs-qa -- eval --out examples/docs-qa/ragas.json
```

Any OpenAI-compatible endpoint works the same way; point `OPENAI_BASE_URL` at it and set
`OPENAI_MODEL` to one of its models. Every command then uses that model, unless you pick another
one for a specific job. [OUTPUT.md](OUTPUT.md) was produced through a local gateway to Claude
models:

```sh
export OPENAI_BASE_URL=http://127.0.0.1:8080/v1
export OPENAI_MODEL=claude-sonnet-5-5

# Indexing makes two calls per chunk, so it uses a cheaper model
cargo run -p docs-qa -- index --recreate --metadata-model claude-haiku-5-5 README.md ...
cargo run -p docs-qa -- ask "..."
cargo run -p docs-qa -- eval --out examples/docs-qa/ragas.json
```

Other sources work the same way: a directory is walked for `*.md` files (respecting
`.gitignore`), and anything else is treated as an `llms-full.txt` and split into one page per
top-level heading:

```sh
cargo run -p docs-qa -- index docs/
cargo run -p docs-qa -- index https://example.com/llms-full.txt
```

See [OUTPUT.md](OUTPUT.md) for a real run, and [`ragas.json`](ragas.json) for the dataset it
exported.

## Flags

| Flag | Where | Default | What it does |
| --- | --- | --- | --- |
| `--model <model>` | global | `gpt-5.4-mini` (env `OPENAI_MODEL`) | Model that answers questions, and the default for the other two |
| `--responses-api` | global | off | Use the Responses API instead of chat completions |
| `--collection <name>` | global | `docs-qa` | Qdrant collection to index into and search |
| `--langfuse` | global | off | Also send traces to Langfuse |
| `--recreate` | `index` | off | Drop the collection first instead of upserting |
| `--metadata-model <model>` | `index` | `--model` / `OPENAI_MODEL` | Model that writes chunk titles and keywords (two calls per chunk) |
| `--questions <path>` | `eval` | bundled [`questions.yaml`](questions.yaml) | YAML list of `question` and `ground_truth`; questions must be unique |
| `--out <path>` | `eval` | `ragas.json` | Where to write the RAGAS dataset |
| `--judge-model <model>` | `eval` | `--model` / `OPENAI_MODEL` | Model that grades the answers; a stronger one, such as `gpt-5.4`, grades more reliably |

All three models are served by the same endpoint. Answer and metadata calls go through Swiftide's
`LanguageModelWithBackOff`, which retries rate limits and server errors with exponential backoff
for up to two minutes, so an unreachable endpoint fails after a minute or two rather than hanging.
That decorator does not wrap `StructuredPrompt`, so the judge retries transient errors itself, up
to five times with a linearly growing delay. Retries are silent: the terminal shows only the
final error. Set `RUST_LOG` (for example `RUST_LOG=warn`) to see each retried request and
Swiftide's own logs.

`--responses-api` needs an endpoint whose Responses API replies match `OpenAI`'s schema exactly.
The local Bifrost gateway used for [OUTPUT.md](OUTPUT.md) returns `"input_tokens_details": null`,
which the underlying `async-openai` client rejects, so that run uses chat completions (the
default).

## How the evaluation works

`eval` builds the same query pipeline as `ask`, with `evaluate_with(Ragas)` added. Swiftide's
`Ragas` evaluator records each question's retrieved contexts and answer, and exports them,
together with the ground truths from `questions.yaml`, as JSON for the Python
[ragas](https://docs.ragas.io) toolkit. It does not compute scores itself.

So that you see numbers straight away, a small LLM judge splits the ground truth and the
answer into claims and checks each one. The scores are plain ratios over those checks, modelled
on the RAGAS metrics of the same names:

- **faithful**: share of the answer's claims that the reranked context supports
- **answer recall**: share of the ground truth's claims that the answer states
- **context recall**: share of the ground truth's claims that the reranked context supports

`ctx` is the number of chunks the LLM saw after reranking. Note that `ragas.json` holds all
20 retrieved candidates per question: the evaluator records contexts at retrieval, before the
reranker narrows them down to ten. The reranker scores each candidate as its page, the chunk
title written during indexing, and the chunk text, so that a chunk which is mostly code still
ranks by what it is about. For the same reason, the title prompt asks for a code-only chunk to
be named after the API it demonstrates: the README's `#[swiftide::tool]` example used to be
titled after what the example tool does ("Swiftide Agent Tool for Explaining Concepts with
OpenAI"), and the reranker dropped it for the tool-macro question. It is now titled after the
macro and ranks among the kept chunks.

Swiftide exports the dataset in hash-map order, which changes from run to run; `eval` sorts the
rows back into the order of the questions file, so exports of different runs can be diffed.

## Code layout

| File | Responsibility |
| --- | --- |
| `src/main.rs` | CLI parsing and wiring |
| `src/docs.rs` | What a documentation page is; title detection and `llms-full.txt` splitting |
| `src/loader.rs` | The custom `Loader`: one `TextNode` per page, with the page title as metadata |
| `src/index.rs` | The indexing pipeline |
| `src/store.rs` | Qdrant setup with a dense and a sparse vector, and collection checks |
| `src/query.rs` | The query pipeline: hybrid search, rerank, answer; plus the `Answer` view |
| `src/retrieval.rs` | Workaround that restores chunk text returned by Qdrant (see below) |
| `src/eval.rs` | The evaluation run with the `Ragas` evaluator |
| `src/judge.rs` | The claim-based LLM judge and its scores |
| `src/models.rs` | FastEmbed models, and chat models on an OpenAI-compatible endpoint |
| `src/render.rs` | Terminal output |
| `src/telemetry.rs` | Logging, and the optional Langfuse layer |

`src/retrieval.rs` exists because, in Swiftide 0.33, Qdrant retrieval returns each chunk's
content `Debug`-quoted (wrapped in `"` with escaped newlines). The wrapper restores the original
markdown before reranking, answering and the Ragas export see it.

## Anchors

The source is marked with mdBook anchors (`// ANCHOR: name`) for the documentation:

| Anchor | File | Shows |
| --- | --- | --- |
| `loader` | `src/loader.rs` | Implementing `Loader` (`type Output`, `into_stream` returning an `IndexingStream`) |
| `metadata_prompts` | `src/index.rs` | Plain-text prompt templates for `MetadataTitle` and `MetadataKeywords` (`{{ node.chunk }}`, `{{ node.metadata.Page }}`) |
| `pipeline` | `src/index.rs` | The indexing pipeline: chunk, LLM metadata, dense and sparse embeddings, Qdrant |
| `store` | `src/store.rs` | Configuring Qdrant with a dense and a sparse vector |
| `answer_templates` | `src/query.rs` | The answer prompt (`{{ question }}`, `{{ documents }}`), the per-document template (`{{ metadata.Page }}`, `{{ content }}`) and the reranker's document template |
| `answerer` | `src/query.rs` | The FastEmbed reranker (scoring page, chunk title and text) and `answers::Simple` using those templates |
| `hybrid` | `src/query.rs` | A query pipeline with the `HybridSearch` strategy |
| `query_pipeline` | `src/query.rs` | Embedding the question twice, retrieving, reranking, answering |
| `rerank` | `src/query.rs` | Reranking as a response transformer (nested in `query_pipeline`) |
| `eval` | `src/eval.rs` | `evaluate_with(Ragas)`, `query_all` and the JSON export |
| `judge_schema` | `src/judge.rs` | The structured output types for the judge |
| `judge` | `src/judge.rs` | A templated prompt sent with `StructuredPrompt::structured_prompt` |
| `llm` | `src/models.rs` | Swiftide's `OpenAI` client on any OpenAI-compatible endpoint (`OPENAI_BASE_URL`, `OPENAI_API_KEY`), wrapped in backoff |
| `render` | `src/render.rs` | Printing the answer with its sources |
| `langfuse` | `src/telemetry.rs` | Adding the Langfuse tracing layer next to terminal logging |
