//! The `loaders` module provides functionality for loading files from a specified directory.
//! It includes the `FileLoader` struct which is used to filter and stream files based on their
//! extensions.
//!
//! This module is a part of the Swiftide project, designed for asynchronous file indexing and
//! processing. The `FileLoader` struct is re-exported for ease of use in other parts of the
//! project.

pub mod file_loader;

pub use file_loader::FileLoader;
