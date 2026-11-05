# Integrations cheat sheet (Swiftide 0.33)

Every integration sits behind a feature flag on the `swiftide` crate and lives under
`swiftide::integrations::<name>`. Every snippet is copied from compiled code; the source file is
named above each one.

| Feature | Module | Main types |
| --- | --- | --- |
| `openai` | `integrations::openai` | `OpenAI`, `OpenAIConfig`, `Options` |
| `anthropic` | `integrations::anthropic` | `Anthropic` |
| `gemini` | `integrations::gemini` | `Gemini` |
| `groq` | `integrations::groq` | `Groq` |
| `ollama` | `integrations::ollama` | `Ollama` |
| `open-router` | `integrations::open_router` | `OpenRouter` |
| `dashscope` | `integrations::dashscope` | `Dashscope`, `DashscopeBuilder` |
| `aws-bedrock` | `integrations::aws_bedrock_v2` | `AwsBedrock` |
| `fastembed` | `integrations::fastembed` | `FastEmbed`, `Rerank` |
| `qdrant` | `integrations::qdrant` | `Qdrant` (re-exports `qdrant_client`) |
| `lancedb` | `integrations::lancedb` | `LanceDB` |
| `pgvector` | `integrations::pgvector` | `PgVector` |
| `redis` | `integrations::redis` | `Redis` (node cache and storage) |
| `redb` | `integrations::redb` | `Redb` (node cache) |
| `duckdb` | `integrations::duckdb` | `Duckdb` (storage, retrieval and cache) |
| `tree-sitter` | `integrations::treesitter` | `SupportedLanguages`, `ChunkCode`, `MetadataQACode` |
| `langfuse` | `swiftide::langfuse` | `LangfuseLayer` |

No feature needed: `swiftide::indexing::persist::MemoryStorage` for tests and small demos.

## OpenAI

`examples/query_pipeline.rs`:

```rust
let openai_client = integrations::openai::OpenAI::builder()
    .default_embed_model("text-embedding-3-large")
    .default_prompt_model("gpt-4o")
    .build()?;
```

Reads `OPENAI_API_KEY` by default. `OpenAI` implements `SimplePrompt`, `ChatCompletion`,
`EmbeddingModel` and `StructuredPrompt`; clone it freely.

Builder options seen in the examples: `.stream_full(false)` (stream deltas only),
`.use_responses_api(true)` (Responses API instead of Chat Completions), `.default_options(..)`,
`.client(async_openai::Client::with_config(config))`.

### Any OpenAI-compatible endpoint

Gateways, local servers and other providers that speak the OpenAI protocol use the same client
with an `OpenAIConfig`. Add `async-openai` at the version Swiftide uses (0.41) to build the inner
client. `examples/workspace-architect/src/llm.rs`:

```rust
pub fn client(&self, model: &ModelArgs) -> Result<OpenAI> {
    let config = OpenAIConfig::new()
        .with_api_base(&self.base_url)
        .with_api_key(&self.api_key);

    Ok(OpenAI::builder()
        .client(async_openai::Client::with_config(config))
        .default_prompt_model(&model.model)
        .default_options(Options::builder().max_completion_tokens(MAX_OUTPUT_TOKENS))
        .use_responses_api(model.responses_api)
        .build()?)
}
```

Set `max_completion_tokens` explicitly: some gateways cap an unset limit at 4096 tokens, and a
truncated answer looks complete.

### Retries with backoff

Wrap any client in `LanguageModelWithBackOff` for rate limits and transient errors, which matter
when indexing fires many prompts concurrently. `examples/docs-qa/src/models.rs`:

```rust
pub type RetryingModel = LanguageModelWithBackOff<OpenAI>;
```

```rust
pub fn retrying_model(&self, model: &str) -> Result<RetryingModel> {
    let backoff = BackoffConfiguration {
        initial_interval_sec: 5,
        max_elapsed_time_sec: 120,
        ..BackoffConfiguration::default()
    };
    Ok(LanguageModelWithBackOff::new(self.model(model)?, backoff))
}
```

Both types come from `swiftide::indexing`. `examples/ask-your-codebase/src/llm.rs` returns the
wrapped client as `Arc<dyn SimplePrompt>`.

## Other LLM providers

`examples/index_ollama.rs`:

```rust
let ollama_client = integrations::ollama::Ollama::default()
    .with_default_prompt_model("llama3.1")
    .to_owned();
```

`examples/index_groq.rs`:

```rust
let groq_client = integrations::groq::Groq::builder()
    .default_prompt_model("llama3-8b-8192")
    .to_owned()
    .build()?;
```

`examples/hello_agents.rs`:

```rust
let openai = swiftide::integrations::gemini::Gemini::builder()
    .default_embed_model("gemini-embedding-exp-03-07")
    .default_prompt_model("gemini-2.0-flash")
    .build()?;
```

`examples/aws_bedrock.rs`:

```rust
let aws_bedrock = integrations::aws_bedrock_v2::AwsBedrock::builder()
    .default_prompt_model("global.anthropic.claude-haiku-4-5-20251001-v1:0")
    .build()?;
```

Anthropic follows the same pattern: `integrations::anthropic::Anthropic::builder()` with
`.default_prompt_model(..)`.

## FastEmbed (local embeddings and reranking)

Runs on the CPU, needs no API key, downloads models on first use. Pin the `fastembed` crate to the
major version Swiftide uses (5) if you pick a model yourself.

`examples/docs-qa/src/models.rs` (default dense model, BGE small, 384 dimensions; and SPLADE
sparse):

```rust
Ok(Self {
    dense: FastEmbed::try_default()?,
    sparse: FastEmbed::try_default_sparse()?,
})
```

`examples/ask-your-codebase/src/embedding.rs` (a code model, 768 dimensions):

```rust
pub fn local_model() -> Result<FastEmbed> {
    let model = TextEmbedding::try_new(
        InitOptions::new(EmbeddingModel::JinaEmbeddingsV2BaseCode).with_max_length(MAX_TOKENS),
    )?;

    FastEmbed::builder()
        .embedding_model(model)
        .batch_size(BATCH_SIZE)
        .build()
}
```

Reranker, `examples/reranking.rs`:

```rust
let fastembed = fastembed::FastEmbed::builder().batch_size(10).build()?;
let reranker = fastembed::Rerank::builder().top_k(5).build()?;
```

## Qdrant

Connects over gRPC on port 6334 (`QDRANT_URL`, optional `QDRANT_API_KEY`).
`examples/docs-qa/src/store.rs`:

```rust
let qdrant = Qdrant::builder()
    .batch_size(BATCH_SIZE)
    .vector_size(DENSE_VECTOR_SIZE)
    // One dense and one sparse vector, both over the chunk *and* its metadata
    .with_vector(EmbeddedField::Combined)
    .with_sparse_vector(EmbeddedField::Combined)
    .collection_name(&collection.0)
    .build()?;
```

The raw client is available as `qdrant.client()` (for example `health_check`,
`collection_exists`, `delete_collection`).

## LanceDB and Redb (embedded, no server)

`examples/ask-your-codebase/src/codebase.rs`:

```rust
/// Remembers which files were fully indexed, so unchanged files are skipped next time.
pub fn node_cache(&self) -> Result<Redb> {
    std::fs::create_dir_all(&self.data_dir)?;

    Redb::builder()
        .database_path(self.data_dir.join("cache.redb"))
        .table_name("indexed_files")
        .build()
}

/// Embedded vector store with one vector per chunk and the source path next to it.
pub fn vector_store(&self) -> Result<LanceDB> {
    let uri = self.lancedb_dir();

    LanceDB::builder()
        .uri(uri.to_str().context("Index path is not valid UTF-8")?)
        .table_name("code_chunks")
        .vector_size(embedding::DIMENSIONS)
        .with_vector(EmbeddedField::Combined)
        .with_metadata(PATH_FIELD)
        .build()
}
```

`with_metadata(field)` stores that metadata field in its own column so retrieval returns it.

## pgvector and Redis

`examples/index_md_into_pgvector.rs`:

```rust
let pgv_storage = PgVector::builder()
    .db_url(pgv_db_url)
    .vector_size(384)
    .with_vector(EmbeddedField::Combined)
    .with_metadata(METADATA_QA_TEXT_NAME)
    .table_name("swiftide_pgvector_test".to_string())
    .build()
    .unwrap();
```

`examples/index_codebase.rs`:

```rust
.filter_cached(Redis::try_from_url(redis_url, "swiftide-examples")?)
```

## Langfuse tracing

Feature `langfuse`. `LangfuseLayer::default()` reads `LANGFUSE_PUBLIC_KEY`, `LANGFUSE_SECRET_KEY`
and `LANGFUSE_URL`. Flush before exit. `examples/release-notes-agent/src/telemetry.rs`:

```rust
tracing_subscriber::registry()
    .with(logs)
    .with(
        langfuse
            .clone()
            // Langfuse data is logged at debug level; only send Swiftide's spans, not
            // those of every dependency.
            .map(|layer| {
                layer
                    .with_filter(Targets::new().with_target("swiftide", LevelFilter::DEBUG))
            }),
    )
    .try_init()?;
```

```rust
pub async fn flush(&self) -> Result<()> {
    if let Some(langfuse) = &self.langfuse {
        langfuse.flush().await?;
    }
    Ok(())
}
```

## TLS

Swiftide's HTTP clients use rustls and need no setup: each client uses the process-wide rustls
`CryptoProvider` when the application installed one, and otherwise its own built-in provider
(aws-lc-rs for OpenAI-compatible clients, ring for Anthropic). Anthropic uses rustls even when
another crate (such as `fastembed`) enables native TLS on reqwest 0.12.

Redis over `rediss://` needs a process-wide provider. If none is installed when it first
connects, Swiftide installs aws-lc-rs; install your own before the first Redis call to use a
different one.

Other crates in your application that call `rustls::ClientConfig::builder()` rely on the
process-wide provider and panic when the build enables both rustls providers, for example
`openai` with `qdrant`, `fastembed` or `lancedb`. In that case install one at the start of `main`
(requires a direct `rustls` dependency with the `aws_lc_rs` feature):

```rust
let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
```
