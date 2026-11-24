//! The Qdrant collection that holds both dense and sparse vectors for every chunk.

use anyhow::{Context as _, Result};
use swiftide::{
    indexing::EmbeddedField,
    integrations::qdrant::{Qdrant, qdrant_client::qdrant::CountPointsBuilder},
};

/// `FastEmbed`'s default dense model (BGE small) produces 384 dimensional vectors.
const DENSE_VECTOR_SIZE: u64 = 384;
const BATCH_SIZE: usize = 64;

const QDRANT_HINT: &str = "Is Qdrant running? Start it with:

    docker run -d -p 6333:6333 -p 6334:6334 qdrant/qdrant

Swiftide talks to Qdrant over gRPC (port 6334); set QDRANT_URL to use another instance.";

/// Name of the Qdrant collection, e.g. `docs-qa`.
#[derive(Debug, Clone)]
pub struct Collection(String);

impl std::fmt::Display for Collection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for Collection {
    fn from(name: String) -> Self {
        Self(name)
    }
}

/// Connects to Qdrant, failing early with a helpful hint when it is not reachable.
pub async fn connect(collection: &Collection) -> Result<Qdrant> {
    // ANCHOR: store
    let qdrant = Qdrant::builder()
        .batch_size(BATCH_SIZE)
        .vector_size(DENSE_VECTOR_SIZE)
        // One dense and one sparse vector, both over the chunk *and* its metadata
        .with_vector(EmbeddedField::Combined)
        .with_sparse_vector(EmbeddedField::Combined)
        .collection_name(&collection.0)
        .build()?;
    // ANCHOR_END: store

    qdrant.client().health_check().await.context(QDRANT_HINT)?;

    Ok(qdrant)
}

/// Removes the collection so the next index run starts from scratch.
pub async fn drop_collection(qdrant: &Qdrant, collection: &Collection) -> Result<()> {
    if qdrant.client().collection_exists(&collection.0).await? {
        qdrant.client().delete_collection(&collection.0).await?;
    }
    Ok(())
}

/// Fails with a hint to run `index` first when the collection does not exist.
pub async fn ensure_indexed(qdrant: &Qdrant, collection: &Collection) -> Result<()> {
    anyhow::ensure!(
        qdrant.client().collection_exists(&collection.0).await?,
        "Collection `{}` does not exist yet; run `docs-qa index <source>` first",
        collection.0
    );
    Ok(())
}

/// Number of chunks stored in the collection.
pub async fn chunk_count(qdrant: &Qdrant, collection: &Collection) -> Result<u64> {
    let count = qdrant
        .client()
        .count(CountPointsBuilder::new(&collection.0).exact(true))
        .await?
        .result
        .map_or(0, |result| result.count);

    Ok(count)
}
