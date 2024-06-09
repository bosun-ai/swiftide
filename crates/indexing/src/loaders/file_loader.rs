use futures_util::{stream, StreamExt};
use std::path::PathBuf;

use crate::{ingestion_node::IngestionNode, ingestion_pipeline::IngestionStream, traits::Loader};

pub struct FileLoader {
    pub(crate) path: PathBuf,
    pub(crate) extensions: Vec<String>,
}

impl FileLoader {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            extensions: vec![],
        }
    }

    /// Add extensions to the loader
    ///
    /// # Arguments
    /// * `extensions` - A list of extensions to add without the leading dot
    pub fn with_extensions(mut self, extensions: &[&str]) -> Self {
        self.extensions
            .extend(extensions.iter().map(ToString::to_string));
        self
    }

    /// Debug method
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

        stream::iter(file_paths).boxed()
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
