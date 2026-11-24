//! Streaming indexing pipelines for Swiftide.
//!
//! This crate provides the indexing [`Pipeline`], the [`loaders::FileLoader`], built-in
//! [`transformers`] (chunking, embedding, LLM generated metadata) and the in-memory
//! [`persist::MemoryStorage`]. The traits these build on live in `swiftide-core`; storage,
//! models and code-aware transformers come from `swiftide-integrations`.
//!
//! The [`swiftide`](https://docs.rs/swiftide) facade re-exports this crate together with the core
//! indexing types as `swiftide::indexing`. Prefer that unless you want a minimal dependency.
//!
//! Guide: <https://swiftide.rs/indexing/>
//!
//! ```no_run
//! use swiftide_indexing::{
//!     Pipeline, loaders::FileLoader, persist::MemoryStorage, transformers::ChunkMarkdown,
//! };
//!
//! # async fn run() -> anyhow::Result<()> {
//! Pipeline::from_loader(FileLoader::new("README.md").with_extensions(&["md"]))
//!     .then_chunk(ChunkMarkdown::from_chunk_range(20..2048))
//!     .then_store_with(MemoryStorage::default())
//!     .run()
//!     .await?;
//! # Ok(())
//! # }
//! ```
// show feature flags in the generated documentation
// https://doc.rust-lang.org/rustdoc/unstable-features.html#extensions-to-the-doc-attribute
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, doc(auto_cfg))]
#![doc(html_logo_url = "https://github.com/bosun-ai/swiftide/raw/master/images/logo.png")]

pub mod loaders;
pub mod persist;
pub mod transformers;

mod pipeline;
pub use pipeline::Pipeline;
