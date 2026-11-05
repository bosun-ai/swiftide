//! Indexing: docs -> pages -> chunks -> LLM metadata -> dense + sparse embeddings -> Qdrant.

use anyhow::Result;
use swiftide::{
    indexing::{
        self,
        transformers::{ChunkMarkdown, Embed, MetadataKeywords, MetadataTitle, SparseEmbed},
    },
    integrations::qdrant::Qdrant,
};

use crate::{
    loader::DocsLoader,
    models::{Embedders, RetryingModel},
};

/// Chunks smaller than this are mostly badges and stray headings; they are dropped.
const MIN_CHUNK_CHARS: usize = 120;
const MAX_CHUNK_CHARS: usize = 1500;
const EMBED_BATCH_SIZE: usize = 32;

// ANCHOR: metadata_prompts
// Swiftide's default metadata prompts show the expected format inside a code fence, and models
// tend to copy the fence into the metadata. These ask for plain text and use the page title.
// A code-only chunk is titled by the API it shows, because the reranker scores the title too.
const TITLE_PROMPT: &str = "Write a short, descriptive title (at most 10 words) for this \
excerpt from the \"{{ node.metadata.Page }}\" documentation. If the excerpt is mostly code, \
name the API it demonstrates (a macro, trait or builder method), not what the example does. \
Reply with the title only: no quotes, no markdown, no code fences.

{{ node.chunk }}";

const KEYWORDS_PROMPT: &str = "List up to 8 keywords for this excerpt from the \
\"{{ node.metadata.Page }}\" documentation: names of types, crates, features, tools, commands \
and concepts that literally appear in it. Reply with one comma-separated line only: no \
numbering, no markdown, no code fences.

{{ node.chunk }}";
// ANCHOR_END: metadata_prompts

pub async fn index(
    loader: DocsLoader,
    llm: &RetryingModel,
    embedders: &Embedders,
    qdrant: &Qdrant,
) -> Result<()> {
    // ANCHOR: pipeline
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
    // ANCHOR_END: pipeline
}
