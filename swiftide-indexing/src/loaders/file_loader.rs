//! Load files from a directory
use anyhow::Context as _;
use std::path::{Path, PathBuf};
use swiftide_core::{indexing::IndexingStream, indexing::Node, Loader};

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
                Node {
                    path: entry,
                    chunk: content,
                    original_size,
                    ..Default::default()
                }
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
            exts.iter().any(|e| e == ext.to_string_lossy().as_ref())
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
                let original_size = content.len();
                Ok(Node {
                    path: entry.path().into(),
                    chunk: content,
                    original_size,
                    ..Default::default()
                })
            });

        IndexingStream::iter(files)
    }

    fn into_stream_boxed(self: Box<Self>) -> IndexingStream {
        self.into_stream()
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
