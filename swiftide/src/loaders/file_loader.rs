use crate::{ingestion::IngestionNode, ingestion::IngestionStream, Loader};
use std::path::PathBuf;

/// The `FileLoader` struct is responsible for loading files from a specified directory,
/// filtering them based on their extensions, and creating a stream of these files for further processing.
pub struct FileLoader {
    pub(crate) path: PathBuf,
    pub(crate) extensions: Vec<String>,
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
            extensions: vec![],
        }
    }

    /// Adds extensions to the loader.
    ///
    /// # Arguments
    /// * `extensions` - A list of extensions to add without the leading dot.
    ///
    /// # Returns
    /// The `FileLoader` instance with the added extensions.
    pub fn with_extensions(mut self, extensions: &[&str]) -> Self {
        self.extensions
            .extend(extensions.iter().map(ToString::to_string));
        self
    }

    /// Lists the nodes (files) that match the specified extensions.
    ///
    /// # Returns
    /// A vector of `IngestionNode` representing the matching files.
    ///
    /// # Panics
    /// This method will panic if it fails to read a file's content.
    pub fn list_nodes(&self) -> Vec<IngestionNode> {
        ignore::Walk::new(&self.path)
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().map(|ft| ft.is_file()).unwrap_or(false))
            .filter(move |entry| {
                let extensions = self.extensions.clone();

                entry
                    .path()
                    .extension()
                    .map(|ext| extensions.contains(&ext.to_string_lossy().to_string()))
                    .unwrap_or(false)
            })
            .map(|entry| entry.into_path())
            .map(|entry| {
                tracing::debug!("Reading file: {:?}", entry);
                let content = std::fs::read_to_string(&entry).unwrap();
                IngestionNode {
                    path: entry,
                    chunk: content,
                    ..Default::default()
                }
            })
            .collect()
    }
}

impl Loader for FileLoader {
    /// Converts the `FileLoader` into a stream of `IngestionNode`.
    ///
    /// # Returns
    /// An `IngestionStream` representing the stream of files.
    ///
    /// # Errors
    /// This method will return an error if it fails to read a file's content.
    fn into_stream(self) -> IngestionStream {
        let file_paths = ignore::Walk::new(self.path)
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().map(|ft| ft.is_file()).unwrap_or(false))
            .filter(move |entry| {
                let extensions = self.extensions.clone();

                entry
                    .path()
                    .extension()
                    .map(|ext| extensions.contains(&ext.to_string_lossy().to_string()))
                    .unwrap_or(false)
            })
            .map(|entry| entry.into_path())
            .map(|entry| {
                let content = std::fs::read_to_string(&entry)?;
                tracing::debug!("Reading file: {:?}", entry);
                Ok(IngestionNode {
                    path: entry,
                    chunk: content,
                    ..Default::default()
                })
            });

        IngestionStream::iter(file_paths)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_with_extensions() {
        let loader = FileLoader::new("/tmp").with_extensions(&["rs"]);
        assert_eq!(loader.extensions, vec!["rs".to_string()]);
    }
}
