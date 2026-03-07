//! Stream data from parquet files
use std::path::PathBuf;

use derive_builder::Builder;

pub mod loader;

/// Stream data from parquet files on a single column
///
/// Provide a path, column and optional batch size. The column must be of type `StringArray`. Then
/// the column is loaded into the chunks of the Node.
///
/// # Panics
///
/// The loader can panic during initialization if anything with parquet or arrow fails before
/// starting the stream.
#[derive(Debug, Clone, Builder)]
#[builder(setter(into, strip_option))]
pub struct Parquet {
    path: PathBuf,
    column_name: String,
    #[builder(default = "1024")]
    batch_size: usize,
}

impl Parquet {
    pub fn builder() -> ParquetBuilder {
        ParquetBuilder::default()
    }
}
