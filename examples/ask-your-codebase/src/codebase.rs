//! A codebase on disk and the index derived from it.
//!
//! Everything the tool derives lives next to the code, in `<root>/.ask-your-codebase/`:
//! a Redb node cache that makes re-indexing incremental, and an embedded `LanceDB` table that
//! holds the chunks and their vectors.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use swiftide::{
    indexing::EmbeddedField,
    integrations::{lancedb::LanceDB, redb::Redb},
};

use crate::embedding;

/// Directory, relative to the codebase root, that holds the cache and the vector store.
const DATA_DIR: &str = ".ask-your-codebase";
/// Metadata field with the path of the source file, relative to the codebase root.
pub const PATH_FIELD: &str = "path";

#[derive(Debug, Clone)]
pub struct Codebase {
    root: PathBuf,
    data_dir: PathBuf,
}

impl Codebase {
    /// Opens the codebase rooted at `path`.
    ///
    /// The root is canonicalized so that cache keys (which include the file path) are the same
    /// no matter how the path was spelled on the command line.
    pub fn at(path: &Path) -> Result<Self> {
        let root = path
            .canonicalize()
            .with_context(|| format!("Codebase not found at {}", path.display()))?;
        let data_dir = root.join(DATA_DIR);

        Ok(Self { root, data_dir })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The path of a file in this codebase, relative to its root.
    pub fn relative_path<'a>(&self, path: &'a Path) -> &'a Path {
        path.strip_prefix(&self.root).unwrap_or(path)
    }

    pub fn is_indexed(&self) -> bool {
        self.lancedb_dir().exists()
    }

    // ANCHOR: storage
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
    // ANCHOR_END: storage

    fn lancedb_dir(&self) -> PathBuf {
        self.data_dir.join("lancedb")
    }
}
