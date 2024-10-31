//! Load files from a directory
use anyhow::Context as _;
use anyhow::Result;
use async_trait::async_trait;
use futures_util::stream::{self, Stream};
use futures_util::TryFutureExt as _;
use futures_util::{StreamExt, TryFutureExt, TryStreamExt};
use itertools::Itertools;
use std::path::{Path, PathBuf};
use swiftide_core::{
    indexing::{IndexingStream, Node},
    AsyncLoader, Loader,
};

/// The `FileLoader` struct is responsible for loading files from a specified directory,
/// filtering them based on their extensions, and creating a stream of these files for further processing.
///
/// # Example
///
/// ```no_run
/// // Create a pipeline that loads the current directory
/// // and indexes all files with the ".rs" extension.
/// # use swiftide_indexing as indexing;
/// # use swiftide_indexing::loaders::FileLoader;
/// indexing::Pipeline::from_loader(FileLoader::new(".").with_extensions(&["rs"]));
/// ```
#[derive(Clone, Debug)]
pub struct FileLoader {
    pub(crate) path: PathBuf,
    pub(crate) extensions: Option<Vec<String>>,
}

impl FileLoader {
    /// Creates a new `FileLoader` with the specified path.
    ///
    /// # Arguments
    /// * `path` - The path to the directory to load files from.
    ///
    /// # Returns
    /// A new instance of `FileLoader`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            extensions: None,
        }
    }

    /// Adds extensions to the loader.
    ///
    /// # Arguments
    /// * `extensions` - A list of extensions to add without the leading dot.
    ///
    /// # Returns
    /// The `FileLoader` instance with the added extensions.
    #[must_use]
    pub fn with_extensions(mut self, extensions: &[impl AsRef<str>]) -> Self {
        self.extensions = Some(
            self.extensions
                .unwrap_or_default()
                .into_iter()
                .chain(extensions.iter().map(|ext| ext.as_ref().to_string()))
                .collect(),
        );
        self
    }

    /// Lists the nodes (files) that match the specified extensions.
    ///
    /// # Returns
    /// A vector of `Node` representing the matching files.
    ///
    /// # Panics
    /// This method will panic if it fails to read a file's content.
    pub fn list_nodes(&self) -> Vec<Node> {
        ignore::Walk::new(&self.path)
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_some_and(|ft| ft.is_file()))
            .filter(move |entry| self.file_has_extension(entry.path()))
            .map(ignore::DirEntry::into_path)
            .map(|entry| {
                tracing::debug!("Reading file: {:?}", entry);
                let content = std::fs::read_to_string(&entry).unwrap();
                let original_size = content.len();
                Node::builder()
                    .path(entry)
                    .chunk(content)
                    .original_size(original_size)
                    .build()
                    .expect("Failed to build node")
            })
            .collect()
    }

    // Helper function to check if a file has the specified extension.
    // If no extensions are specified, this function will return true.
    // If the file has no extension, this function will return false.
    fn file_has_extension(&self, path: &Path) -> bool {
        self.extensions.as_ref().map_or(true, |exts| {
            let Some(ext) = path.extension() else {
                return false;
            };
            exts.iter().any(|e| e.as_str() == ext)
        })
    }
}

impl Loader for FileLoader {
    /// Converts the `FileLoader` into a stream of `Node`.
    ///
    /// # Returns
    /// An `IndexingStream` representing the stream of files.
    ///
    /// # Errors
    /// This method will return an error if it fails to read a file's content.
    fn into_stream(self) -> IndexingStream {
        let files = ignore::Walk::new(&self.path)
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_some_and(|ft| ft.is_file()))
            .filter(move |entry| self.file_has_extension(entry.path()))
            .map(|entry| {
                tracing::debug!("Reading file: {:?}", entry);
                let content =
                    std::fs::read_to_string(entry.path()).context("Failed to read file")?;
                let original_size = entry
                    .metadata()
                    .map(|m| m.len())
                    .unwrap_or_else(|_| content.len() as u64);

                Node::builder()
                    .path(entry.path())
                    .chunk(content)
                    .original_size(original_size as usize)
                    .build()
            });

        IndexingStream::iter(files)
    }

    fn into_stream_boxed(self: Box<Self>) -> IndexingStream {
        Loader::into_stream(*self)
    }
}

#[async_trait]
impl AsyncLoader for FileLoader {
    /// Converts the `FileLoader` into a stream of `Node`.
    ///
    /// # Returns
    /// An `IndexingStream` representing the stream of files.
    ///
    /// # Errors
    /// This method will return an error if it fails to read a file's content.
    async fn into_stream(self) -> IndexingStream {
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Node>>(100);

        tokio::spawn(async move {
            for entry in ignore::Walk::new(&self.path)
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_some_and(|ft| ft.is_file()))
                .filter(move |entry| self.file_has_extension(entry.path()))
            {
                tracing::debug!("Reading file: {:?}", entry);
                let Ok(content) = tokio::fs::read_to_string(entry.path()).await else {
                    tx.send(Err(anyhow::anyhow!("Failed to read file")))
                        .await
                        .unwrap();
                    continue;
                };
                let original_size = entry
                    .metadata()
                    .map(|m| m.len())
                    .unwrap_or_else(|_| content.len() as u64);

                if tx
                    .send(
                        Node::builder()
                            .path(entry.path())
                            .chunk(content)
                            .original_size(original_size as usize)
                            .build(),
                    )
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        rx.into()
    }

    async fn into_stream_boxed(self: Box<Self>) -> IndexingStream {
        AsyncLoader::into_stream(*self).await
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_with_extensions() {
        let loader = FileLoader::new("/tmp").with_extensions(&["rs"]);
        assert_eq!(loader.extensions, Some(vec!["rs".to_string()]));
    }
}
