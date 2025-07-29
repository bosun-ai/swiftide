//! PDF document ingestion support for Swiftide
//!
//! This module provides a loader for extracting text content from PDF files
//! and feeding it into the Swiftide indexing pipeline. The loader converts
//! PDF text to Markdown format for better downstream processing.
//!
//! # Features
//!
//! - Text extraction from PDF documents
//! - Conversion to Markdown format for structured processing
//! - Extensible design for future enhancements (tables, images, etc.)
//!
//! # Example
//!
//! ```no_run
//! # use swiftide_integrations::pdf::PdfLoader;
//! # use swiftide_indexing::Pipeline;
//! let loader = PdfLoader::from_path("document.pdf");
//! let pipeline = Pipeline::from_loader(loader);
//! ```

use std::{
    path::{Path, PathBuf},
    fmt,
};
use derive_builder::Builder;

pub mod loader;

/// A loader for PDF documents that extracts text content and converts it to Markdown
///
/// This loader uses the `pdf-extract` crate to extract text from PDF files and
/// formats the output as Markdown. It's designed to be extensible for future
/// enhancements like table extraction and image processing.
///
/// # Example
///
/// ```no_run
/// # use swiftide_integrations::pdf::PdfLoader;
/// # use swiftide_indexing::Pipeline;
/// let loader = PdfLoader::from_path("document.pdf");
/// let pipeline = Pipeline::from_loader(loader);
/// ```
#[derive(Debug, Clone, Builder)]
#[builder(setter(into, strip_option))]
pub struct PdfLoader {
    /// Path to the PDF file to load
    path: PathBuf,
    
    /// Whether to add basic Markdown formatting to the extracted text
    #[builder(default = "true")]
    format_as_markdown: bool,
}

impl PdfLoader {
    /// Creates a new PDF loader for the specified file path
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the PDF file to load
    ///
    /// # Returns
    ///
    /// A new `PdfLoader` instance
    pub fn from_path(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            format_as_markdown: true,
        }
    }

    /// Creates a builder for configuring the PDF loader
    ///
    /// # Returns
    ///
    /// A `PdfLoaderBuilder` for advanced configuration
    pub fn builder() -> PdfLoaderBuilder {
        PdfLoaderBuilder::default()
    }

    /// Sets whether to format the extracted text as Markdown
    ///
    /// # Arguments
    ///
    /// * `enabled` - Whether to enable Markdown formatting
    ///
    /// # Returns
    ///
    /// The modified `PdfLoader` instance
    #[must_use]
    pub fn with_markdown_formatting(mut self, enabled: bool) -> Self {
        self.format_as_markdown = enabled;
        self
    }

    /// Returns the path of the PDF file being loaded
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns whether Markdown formatting is enabled
    pub fn is_markdown_formatting_enabled(&self) -> bool {
        self.format_as_markdown
    }
}

impl fmt::Display for PdfLoader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PdfLoader({})", self.path.display())
    }
}
