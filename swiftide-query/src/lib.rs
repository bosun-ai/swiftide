//! Query pipelines for retrieval augmented generation in Swiftide.
//!
//! This crate provides the query [`Pipeline`] and its built-in steps:
//! [`query_transformers`] (subquestion generation, dense and sparse embedding),
//! [`response_transformers`] (summarizing retrieved documents), [`answers`] (generating the final
//! answer) and [`evaluators`] (Ragas-compatible evaluation). The step traits and query states live
//! in `swiftide-core`; retrievers such as Qdrant, pgvector and `LanceDB` come from
//! `swiftide-integrations`.
//!
//! The [`swiftide`](https://docs.rs/swiftide) facade re-exports this crate together with the core
//! query types as `swiftide::query`. Prefer that unless you want a minimal dependency.
//!
//! Guide: <https://swiftide.rs/querying/>
// show feature flags in the generated documentation
// https://doc.rust-lang.org/rustdoc/unstable-features.html#extensions-to-the-doc-attribute
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, doc(auto_cfg))]
#![doc(html_logo_url = "https://github.com/bosun-ai/swiftide/raw/master/images/logo.png")]

pub mod answers;
mod query;
pub mod query_transformers;
pub mod response_transformers;

pub use query::*;
pub mod evaluators;
