# Pipelines cheat sheet (Swiftide 0.33)

Indexing: `swiftide::indexing`. Querying: `swiftide::query`. Both ship with the `swiftide` crate;
the loaders, embedders and stores behind them need their own feature flags
(`references/integrations.md`). Every snippet is copied from compiled code; the source file is
named above each one.

## Indexing model

- Nodes are `Node<T>`; most pipelines use `TextNode` (`Node<String>`) with `chunk`, `path` and
  `metadata` fields.
- `Pipeline::from_loader(loader)` or `Pipeline::from_stream(stream)` starts a pipeline.
- Steps: `.filter(..)`, `.filter_cached(cache)`, `.then(transformer)`,
  `.then_in_batch(batch_transformer)`, `.then_chunk(chunker)`, `.then_store_with(storage)`, plus
  `.split_by(..)`, `.merge(..)`, `.log_all()`, `.with_concurrency(n)`,
  `.with_default_llm_client(..)`, `.with_embed_mode(..)`.
- Nothing runs until `.run().await`.
- A closure `Fn(TextNode) -> Result<TextNode>` is a transformer.

## Full indexing pipeline: local code RAG

Tree-sitter chunking, LLM-generated questions as metadata, local embeddings, an incremental Redb
cache and an embedded LanceDB store. `examples/ask-your-codebase/src/index.rs`:

```rust
indexing::Pipeline::from_loader(
    FileLoader::new(codebase.root()).with_extensions(&languages.extensions()),
)
.filter(test_code::not_test_files(codebase.clone()))
// Drop unit tests before the cache, which keys on path and content. Changing the content
// after it would make the cache miss on every run.
.then(test_code::without_unit_tests)
.then(files_found.count())
// Skip files whose path and content are unchanged since the last successful run
.filter_cached(codebase.node_cache()?)
.then(files_changed.count())
.then(record_relative_path(codebase.clone()))
// Split along syntax boundaries (functions, impls, ...) with tree-sitter
.then_chunk(ChunkByLanguage::try_new(languages, CHUNK_SIZE)?)
// Ask the LLM which questions each chunk answers; stored as metadata and embedded
.then(
    MetadataQACode::from_client(llm)
        .prompt_template(QUESTIONS_PROMPT)
        .num_questions(QUESTIONS_PER_CHUNK)
        .concurrency(concurrent_requests)
        .build()?,
)
// Embed chunk + metadata locally, in batches
.then_in_batch(Embed::new(embedding::local_model()?).with_batch_size(embedding::BATCH_SIZE))
.then_store_with(codebase.vector_store()?)
.then(chunks_indexed.count())
.run()
.await?;
```

Metadata prompt templates are Tera and see `node` (for example `{{ node.chunk }}`) plus
transformer-specific values such as `{{ questions }}`.

### Closure transformer that writes metadata

`examples/ask-your-codebase/src/index.rs`:

```rust
fn record_relative_path(
    codebase: Codebase,
) -> impl Fn(TextNode) -> Result<TextNode> + Clone + Send + Sync + 'static {
    move |mut node| {
        let path = codebase
            .relative_path(&node.path)
            .to_string_lossy()
            .into_owned();
        node.metadata.insert(PATH_FIELD, path);
        Ok(node)
    }
}
```

### Custom chunker

Implement `ChunkerTransformer`; return an `IndexingStream`. Errors convert with `.into()`.
`examples/ask-your-codebase/src/languages.rs`:

```rust
#[async_trait]
impl ChunkerTransformer for ChunkByLanguage {
    type Input = String;
    type Output = String;

    async fn transform_node(&self, node: TextNode) -> IndexingStream<String> {
        match self.chunker_for(&node.path) {
            Some(chunker) => chunker.transform_node(node).await,
            None => anyhow!("No chunker for {}", node.path.display()).into(),
        }
    }
}
```

Built-in chunkers: `ChunkMarkdown::from_chunk_range(10..2048)`, `ChunkText`,
`ChunkCode::try_for_language_and_chunk_size("rust", 10..2048)` (feature `tree-sitter`; also takes
a `SupportedLanguages` value).

### Custom loader

Implement `Loader` and return an `IndexingStream` of nodes.
`examples/docs-qa/src/loader.rs`:

```rust
impl Loader for DocsLoader {
    type Output = String;

    fn into_stream(self) -> IndexingStream<String> {
        stream::iter(self.sources)
            .then(DocsSource::pages)
            .map_ok(|pages| stream::iter(pages).map(|page| Ok(page_node(page))))
            .try_flatten()
            .boxed()
            .into()
    }

    // Only needed when the loader is used as a `Box<dyn Loader>`.
    fn into_stream_boxed(self: Box<Self>) -> IndexingStream<String> {
        self.into_stream()
    }
}

fn page_node(page: Page) -> TextNode {
    let mut node = TextNode::new(page.markdown);
    node.path = page.origin.into();
    node.metadata.insert(PAGE_TITLE, page.title.as_str());
    node
}
```

## Indexing for hybrid search

Dense and sparse embeddings from FastEmbed, LLM title and keywords as metadata.
`examples/docs-qa/src/index.rs`:

```rust
indexing::Pipeline::from_loader(loader)
    .then_chunk(ChunkMarkdown::from_chunk_range(
        MIN_CHUNK_CHARS..MAX_CHUNK_CHARS,
    ))
    // A short title and keywords per chunk; both end up in the embedded text and help the
    // sparse (keyword) side of hybrid search in particular.
    .then(
        MetadataTitle::from_client(llm.clone())
            .prompt_template(TITLE_PROMPT)
            .build()?,
    )
    .then(
        MetadataKeywords::from_client(llm.clone())
            .prompt_template(KEYWORDS_PROMPT)
            .build()?,
    )
    .then_in_batch(Embed::new(embedders.dense.clone()).with_batch_size(EMBED_BATCH_SIZE))
    .then_in_batch(SparseEmbed::new(embedders.sparse.clone()).with_batch_size(EMBED_BATCH_SIZE))
    .then_store_with(qdrant.clone())
    .run()
    .await
```

Metadata transformers: `MetadataQAText`, `MetadataSummary`, `MetadataTitle`, `MetadataKeywords`,
and `MetadataQACode` (feature `tree-sitter`). Each has `new(client)` and a `from_client(client)`
builder with `.prompt_template(..)`.

## Query model

- `query::Pipeline::default()` uses `SimilaritySingleEmbedding`; pick another with
  `query::Pipeline::from_search_strategy(strategy)`.
- Order: `then_transform_query` (zero or more) → `then_retrieve(store)` →
  `then_transform_response` (zero or more) → `then_answer(answerer)`.
- The query is `Query<states::Pending>`, then `Query<states::Retrieved>`, then
  `Query<states::Answered>`. `pipeline.query(q)` returns the answered query; read `.answer()`,
  `.documents()` and `.original()`. `query_all(questions)` runs many.
- Query transformers: `query_transformers::{GenerateSubquestions, Embed, SparseEmbed}`. Response
  transformers: `response_transformers::Summary`, `fastembed::Rerank`. Answer: `answers::Simple`.

### Similarity search with custom prompts

`examples/ask-your-codebase/src/query.rs`:

```rust
let mut search: SimilaritySingleEmbedding = SimilaritySingleEmbedding::default();
search.with_top_k(top_k);

let pipeline = query::Pipeline::from_search_strategy(search)
    // Rephrase the question into several, to widen what the search can match
    .then_transform_query(
        GenerateSubquestions::builder()
            .client(llm.clone())
            .prompt_template(SUBQUESTIONS_PROMPT.into())
            .build()?,
    )
    // Embed with the same local model that embedded the chunks
    .then_transform_query(Embed::from_client(embedding::local_model()?))
    .then_retrieve(codebase.vector_store()?)
    // Answer the original question from the retrieved code, each chunk under its file path
    .then_answer(
        answers::Simple::builder()
            .client(llm)
            .prompt_template(ANSWER_PROMPT.into())
            .document_template(CHUNK_TEMPLATE)
            .build()?,
    );

let answered = pipeline.query(question).await?;
```

The answer prompt sees `{{ question }}` and `{{ documents }}`; the document template sees
`{{ content }}` and `{{ metadata.<field> }}`:

````rust
const CHUNK_TEMPLATE: &str = r"### {{ metadata.path }}

```
{{ content }}
```
";
````

Reading sources from the answered query:

```rust
impl From<Query<states::Answered>> for Answer {
    /// Keeps the distinct file paths of the retrieved chunks, most relevant first.
    fn from(query: Query<states::Answered>) -> Self {
        let mut sources: Vec<String> = Vec::new();

        for document in query.documents() {
            let Some(path) = document.metadata().get(PATH_FIELD).and_then(|v| v.as_str()) else {
                continue;
            };
            if !sources.iter().any(|source| source == path) {
                sources.push(path.to_owned());
            }
        }

        Self {
            text: query.answer().to_owned(),
            sources,
        }
    }
}
```

### Hybrid search with reranking

`examples/docs-qa/src/query.rs`:

```rust
query::Pipeline::from_search_strategy(
    HybridSearch::default()
        .with_top_n(CANDIDATES)
        .with_top_k(CANDIDATES)
        .to_owned(),
)
```

```rust
pipeline
    // Hybrid search needs both a dense and a sparse embedding of the question
    .then_transform_query(query_transformers::Embed::from_client(
        self.embedders.dense.clone(),
    ))
    .then_transform_query(query_transformers::SparseEmbed::from_client(
        self.embedders.sparse.clone(),
    ))
    .then_retrieve(self.qdrant.clone())
    // A local cross-encoder reorders the fused candidates; only the best few remain
    .then_transform_response(self.reranker.clone())
    .then_answer(self.answerer.clone())
```

```rust
let reranker = Rerank::builder()
    .top_k(CONTEXT_CHUNKS)
    .document_template(Some(RERANK_TEMPLATE.to_string()))
    .build()?;

let answerer = answers::Simple::builder()
    .client(llm)
    .prompt_template(ANSWER_PROMPT.into())
    .document_template(DOCUMENT_TEMPLATE)
    .build()?;
```

A named pipeline type: `query::Pipeline<'static, HybridSearch<Filter>, states::Answered>`.

### Custom retriever

Implement `Retrieve<Strategy>` to wrap or replace a store. `examples/docs-qa/src/retrieval.rs`:

```rust
#[async_trait]
impl Retrieve<HybridSearch<Filter>> for CleanQdrant {
    async fn retrieve(
        &self,
        search_strategy: &HybridSearch<Filter>,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>> {
        let mut retrieved = self.0.retrieve(search_strategy, query).await?;

        for document in retrieved.documents_mut() {
            if let Some(content) = unquote(document.content()) {
                *document = Document::new(content, Some(document.metadata().clone()));
            }
        }

        Ok(retrieved)
    }
}
```

## Evaluation with Ragas

`.evaluate_with(ragas)` on the pipeline records retrievals and answers; export with `to_json()`
for the Python `ragas` toolkit. `examples/docs-qa/src/eval.rs` and `query.rs`:

```rust
let ragas = Ragas::from_prepared_questions(&questions);

let answered = qa
    .evaluated_pipeline(ragas.clone())
    .query_all(ragas.questions().await)
    .await?;

let ragas_json = ragas.to_json().await;
```

```rust
pub fn evaluated_pipeline(&self, ragas: Ragas) -> AnsweringPipeline {
    self.answer_with(Self::hybrid_search().evaluate_with(ragas))
}
```

## Structured prompts

For a typed LLM answer outside a pipeline (`examples/docs-qa/src/judge.rs`):

```rust
let judgement: Judgement =
    retry_transient(|| self.llm.structured_prompt(prompt.clone())).await?;
```

The target type derives `Deserialize` and `JsonSchema` with `#[serde(deny_unknown_fields)]`
(required for OpenAI strict schemas). `DynStructuredPrompt::structured_prompt_dyn` is the
object-safe variant (`examples/structured_prompt.rs`).
