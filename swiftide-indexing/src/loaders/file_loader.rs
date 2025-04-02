//! Load files from a directory
use std::{
    io::Read as _,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use ignore::{DirEntry, Walk};
use swiftide_core::{indexing::IndexingStream, indexing::Node, Loader};
use tracing::{debug_span, instrument, Span};

/// The `FileLoader` struct is responsible for loading files from a specified directory, filtering
/// them based on their extensions, and creating a stream of these files for further processing.
///
/// # Example
///
/// Create a pipeline that loads the current directory and indexes all files with the ".rs"
///
/// ```no_run
/// # use swiftide_indexing as indexing;
/// # use swiftide_indexing::loaders::FileLoader;
/// indexing::Pipeline::from_loader(FileLoader::new(".").with_extensions(&["rs"]));
/// ```
#[derive(Clone, Debug)]
pub struct FileLoader {
    pub(crate) root: PathBuf,
    pub(crate) extensions: Option<Vec<String>>,
}

impl FileLoader {
    /// Creates a new `FileLoader` with the specified path.
    ///
    /// # Arguments
    ///
    /// - `root`: The root directory to load files from.
    ///
    /// # Returns
    ///
    /// A new instance of `FileLoader`.
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            extensions: None,
        }
    }

    /// Adds extensions to the loader.
    ///
    /// # Arguments
    ///
    /// - `extensions`: A list of extensions to add without the leading dot.
    ///
    /// # Returns
    ///
    /// The `FileLoader` instance with the added extensions.
    #[must_use]
    pub fn with_extensions(mut self, extensions: &[impl AsRef<str>]) -> Self {
        let existing = self.extensions.get_or_insert_default();
        existing.extend(extensions.iter().map(|ext| ext.as_ref().to_string()));
        self
    }

    /// Lists the nodes (files) that match the specified extensions.
    ///
    /// # Returns
    ///
    /// A vector of `Node` representing the matching files.
    ///
    /// # Panics
    ///
    /// This method will panic if it fails to read a file's content.
    pub fn list_nodes(&self) -> Vec<Node> {
        self.iter().filter_map(Result::ok).collect()
    }

    /// Iterates over the files in the directory
    pub fn iter(&self) -> impl Iterator<Item = anyhow::Result<Node>> {
        Iter::new(&self.root, self.extensions.clone()).fuse()
    }
}

/// An iterator that walks over the files in a directory and loads them.
///
/// This is a private struct that is used to implement the `FileLoader` iterator.
struct Iter {
    /// The walk instance that iterates over the files in the directory.
    walk: Walk,
    /// The extensions to include.
    include_extensions: Option<Vec<String>>,
    /// A span that tracks the current file loader.
    span: Span,
}

impl Iterator for Iter {
    type Item = anyhow::Result<Node>;

    fn next(&mut self) -> Option<Self::Item> {
        let _span = self.span.enter();
        loop {
            // stop the iteration if there are no more entries
            let entry = self.walk.next()?;

            // propagate any errors that occur during the directory traversal
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => return Some(Err(err.into())),
            };

            if let Some(node) = self.load(entry) {
                return Some(node);
            }
        }
    }
}

impl Iter {
    /// Creates a new `Iter` instance.
    fn new(root: &Path, include_extensions: Option<Vec<String>>) -> Self {
        let span = debug_span!("file_loader", root = %root.display());
        tracing::debug!(parent: &span, extensions = ?include_extensions, "Loading files");
        Self {
            walk: Walk::new(root),
            include_extensions,
            span,
        }
    }

    #[instrument(skip_all, fields(path = %entry.path().display()))]
    fn load(&self, entry: DirEntry) -> Option<anyhow::Result<Node>> {
        if entry.file_type().is_some_and(|ft| !ft.is_file()) {
            // Skip directories and non-files
            return None;
        }
        if let Some(extensions) = &self.include_extensions {
            let Some(extension) = entry.path().extension() else {
                tracing::trace!("Skipping file without extension");
                return None;
            };
            let extension = extension.to_string_lossy();
            if !extensions.iter().any(|ext| ext == &extension) {
                tracing::trace!("Skipping file with extension {extension}");
                return None;
            }
        }
        tracing::debug!("Loading file");
        match read_node(&entry) {
            Ok(node) => {
                tracing::debug!(node_id = %node.id(), "Loaded file");
                Some(Ok(node))
            }
            Err(err) => {
                tracing::error!(error = %err, "Failed to load file");
                Some(Err(err))
            }
        }
    }
}

fn read_node(entry: &DirEntry) -> anyhow::Result<Node> {
    // Files might be invalid utf-8, so we need to read them as bytes and convert it lossy, as
    // Swiftide (currently) works internally with strings.
    let mut file = fs_err::File::open(entry.path()).context("Failed to open file")?;
    let mut buf = vec![];
    file.read_to_end(&mut buf).context("Failed to read file")?;
    let content = String::from_utf8_lossy(&buf);

    let original_size = content.len();

    Node::builder()
        .path(entry.path())
        .chunk(content)
        .original_size(original_size)
        .build()
}

impl Loader for FileLoader {
    /// Converts the `FileLoader` into a stream of `Node`.
    ///
    /// # Returns
    ///
    /// An `IndexingStream` representing the stream of files.
    ///
    /// # Errors
    /// This method will return an error if it fails to read a file's content.
    fn into_stream(self) -> IndexingStream {
        IndexingStream::iter(self.iter())
    }

    fn into_stream_boxed(self: Box<Self>) -> IndexingStream {
        self.into_stream()
    }
}

#[cfg(test)]
mod test {

    use tokio_stream::StreamExt as _;

    use super::*;

    #[test]
    fn test_with_extensions() {
        let loader = FileLoader::new("/tmp").with_extensions(&["rs"]);
        assert_eq!(loader.extensions, Some(vec!["rs".to_string()]));
    }

    #[tokio::test]
    async fn test_ignores_invalid_utf8() {
        let tempdir = temp_dir::TempDir::new().unwrap();

        fs_err::write(tempdir.child("invalid.txt"), [0x80, 0x80, 0x80]).unwrap();

        let loader = FileLoader::new(tempdir.path()).with_extensions(&["txt"]);
        let result = loader.into_stream().collect::<Vec<_>>().await;

        assert_eq!(result.len(), 1);

        let first = result.first().unwrap();

        assert_eq!(first.as_ref().unwrap().chunk, "���".to_string());
    }
}
