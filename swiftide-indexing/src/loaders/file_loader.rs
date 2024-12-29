//! Load files from a directory
use anyhow::Context as _;
use chrono::{DateTime, Local};
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
    #[deprecated(note = "Originally a debug method and will be removed in the future")]
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
                let maybe_modified_at = std::fs::metadata(entry.path())
                    .and_then(|meta| meta.modified())
                    .ok();

                let mut builder = Node::builder()
                    .path(entry.path())
                    .chunk(content)
                    .original_size(original_size)
                    .to_owned();

                if let Some(modified_at) = maybe_modified_at {
                    let modified_at: DateTime<Local> = modified_at.into();
                    builder.with_metadata_value("modified_at", serde_json::to_value(modified_at)?);
                }

                builder.build()
            });

        IndexingStream::iter(files)
    }

    fn into_stream_boxed(self: Box<Self>) -> IndexingStream {
        self.into_stream()
    }
}

#[cfg(test)]
mod test {
    use tokio_stream::StreamExt;

    use super::*;

    #[test]
    fn test_with_extensions() {
        let loader = FileLoader::new("/tmp").with_extensions(&["rs"]);
        assert_eq!(loader.extensions, Some(vec!["rs".to_string()]));
    }

    #[tokio::test]
    async fn test_modified_at() {
        let tempdir = temp_dir::TempDir::new().unwrap();
        let file_path = tempdir.path().join("test.txt");
        std::fs::File::create(&file_path).unwrap();

        let loader = FileLoader::new(tempdir.path());

        let nodes = loader
            .into_stream()
            .collect::<Result<Vec<Node>, _>>()
            .await
            .unwrap();

        let expected_modified_at: DateTime<Local> = std::fs::metadata(&file_path)
            .unwrap()
            .modified()
            .unwrap()
            .into();
        assert_eq!(nodes.len(), 1);
        assert_eq!(&nodes[0].path, &file_path);
        assert_eq!(
            nodes[0]
                .metadata
                .get("modified_at")
                .unwrap()
                .as_str()
                .unwrap(),
            expected_modified_at.to_rfc3339()
        );
    }
}
