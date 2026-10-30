//! Contributed tools, context hooks, and executors for Swiftide agents.
//!
//! All capabilities are opt-in.
//! Enabling a feature never registers a tool or changes an agent automatically.
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, doc(auto_cfg))]
#![doc = include_str!("../README.md")]
#![doc = "## Cargo features"]
#![doc = document_features::document_features!()]
#![warn(missing_docs)]

#[cfg(feature = "lazy-executor")]
pub mod executors;
#[cfg(feature = "context-management")]
pub mod hooks;
#[cfg(any(feature = "planning", feature = "file-tools"))]
pub mod tools;

#[cfg(feature = "context-management")]
mod support;

#[cfg(any(feature = "context-management", feature = "file-tools"))]
mod command;
