//! Indexing: turn the source files of a codebase into searchable, described chunks.

use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use anyhow::Result;
use swiftide::{
    indexing::{
        self, TextNode,
        loaders::FileLoader,
        transformers::{Embed, MetadataQACode},
    },
    traits::SimplePrompt,
};

use crate::{
    codebase::{Codebase, PATH_FIELD},
    embedding,
    languages::{ChunkByLanguage, SourceLanguages},
    test_code,
};

/// Chunks smaller than 10 bytes are dropped, larger than 4096 bytes are split further.
///
/// Large enough to keep most functions, such as a run loop, in one piece.
const CHUNK_SIZE: std::ops::Range<usize> = 10..4096;

/// Questions generated per chunk. Chunks are up to a few functions long; this covers most of
/// what each one does.
const QUESTIONS_PER_CHUNK: usize = 8;

// ANCHOR: questions_prompt
/// Asks for the questions a developer would ask that a chunk answers.
///
/// The default `MetadataQACode` prompt asks generic questions ("What does this code do?").
/// Questions about behaviour, phrased the way a developer would ask them, match real questions
/// much better.
const QUESTIONS_PROMPT: &str = r"
# Task

List the questions a developer could ask about this codebase that the code below answers, each
with a short answer. They are stored with the code to help a search find it.

Focus on behaviour: when and in which order things happen, what starts or ends a process, which
conditions and configuration change the outcome, what happens on errors, and which extension
points (traits, hooks, builders, options) exist and when they apply.

# Constraints

- Write {{ questions }} questions and answers.
- Phrase questions the way a developer would ask them, e.g. 'When does the agent stop?'.
- Name the relevant types, functions and variants in the answers.
- Only use what can be derived from the code.
- Respond only in this format:

Q1: <question>
A1: <answer>

# Code

```
{{ node.chunk }}
```
";
// ANCHOR_END: questions_prompt

/// Indexes the source files of `codebase` written in `languages`.
///
/// Files that were indexed before and did not change are skipped, so running this again after
/// editing a few files only processes those files.
pub async fn index_codebase(
    codebase: &Codebase,
    languages: &SourceLanguages,
    llm: impl SimplePrompt + Clone + 'static,
    concurrent_requests: usize,
) -> Result<IndexReport> {
    let files_found = Tally::default();
    let files_changed = Tally::default();

    // ANCHOR: pipeline
    let stats = indexing::Pipeline::from_loader(
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
    .run()
    .await?;
    // ANCHOR_END: pipeline

    Ok(IndexReport {
        files_found: files_found.total(),
        files_changed: files_changed.total(),
        chunks_indexed: stats.nodes_stored,
        elapsed: stats.duration().unwrap_or_default(),
    })
}

// ANCHOR: path_metadata
/// Stores the file path, relative to the codebase root, as metadata on the node.
///
/// The vector store keeps this field next to each chunk, so answers can cite their sources.
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
// ANCHOR_END: path_metadata

/// What a single indexing run did.
#[derive(Debug)]
pub struct IndexReport {
    files_found: u64,
    files_changed: u64,
    chunks_indexed: u64,
    elapsed: Duration,
}

impl fmt::Display for IndexReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Indexed {} chunks from {} new or changed files ({} files found, {} unchanged) in {:.1}s",
            self.chunks_indexed,
            self.files_changed,
            self.files_found,
            self.files_found.saturating_sub(self.files_changed),
            self.elapsed.as_secs_f64()
        )
    }
}

/// Counts the nodes that pass a point in the pipeline.
#[derive(Clone, Default)]
struct Tally(Arc<AtomicU64>);

impl Tally {
    /// A pass-through transformer that counts every node it sees.
    fn count(&self) -> impl Fn(TextNode) -> Result<TextNode> + Clone + Send + Sync + 'static {
        let tally = Arc::clone(&self.0);
        move |node| {
            tally.fetch_add(1, Ordering::Relaxed);
            Ok(node)
        }
    }

    fn total(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}
