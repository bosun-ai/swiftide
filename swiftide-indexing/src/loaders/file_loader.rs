//! Load files from a directory
use std::{
    io::Read as _,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
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
        self.iter().filter_map(Result::ok).collect()
    }

    /// Iterates over the files in the directory
    pub fn iter(&self) -> impl Iterator<Item = anyhow::Result<Node>> {
        let path = self.path.clone();
        let extensions = self.extensions.clone();

        ignore::Walk::new(path)
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_some_and(|ft| ft.is_file()))
            .filter(move |entry| file_has_extension(extensions.as_deref(), entry.path()))
            .map(move |entry| {
                tracing::debug!("Reading file: {:?}", entry);

                // Files might be invalid utf-8, so we need to read them as bytes and convert it
                // lossy, as Swiftide (currently) works internally with strings.
                let mut file = std::fs::File::open(entry.path()).context("Failed to open file")?;
                let mut buf = vec![];
                file.read_to_end(&mut buf).context("Failed to read file")?;
                let content = String::from_utf8_lossy(&buf);

                let original_size = content.len();

                Node::builder()
                    .path(entry.path())
                    .chunk(content)
                    .original_size(original_size)
                    .build()
            })
    }
}

// Helper function to check if a file has the specified extension.
// If no extensions are specified, this function will return true.
// If the file has no extension, this function will return false.
fn file_has_extension(extensions: Option<&[impl AsRef<str>]>, path: &Path) -> bool {
    extensions.as_ref().is_none_or(|exts| {
        let Some(ext) = path.extension() else {
            return false;
        };
        exts.iter()
            .any(|e| e.as_ref() == ext.to_string_lossy().as_ref())
    })
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

        std::fs::write(tempdir.child("invalid.txt"), [0x80, 0x80, 0x80]).unwrap();

        let loader = FileLoader::new(tempdir.path()).with_extensions(&["txt"]);
        let result = loader.into_stream().collect::<Vec<_>>().await;

        assert_eq!(result.len(), 1);

        let first = result.first().unwrap();

        assert_eq!(first.as_ref().unwrap().chunk, "���".to_string());
    }
}
