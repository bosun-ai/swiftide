//! Core traits, types and pipeline plumbing shared by all Swiftide crates.
//!
//! Most users should depend on the [`swiftide`](https://docs.rs/swiftide) facade crate, which
//! re-exports this crate's public API under `swiftide::traits`, `swiftide::chat_completion`,
//! `swiftide::prompt`, `swiftide::indexing` and `swiftide::query`. Depend on `swiftide-core`
//! directly when building your own integration and you only need the contracts.
//!
//! What lives here:
//! - Indexing contracts: [`Loader`], [`Transformer`], [`BatchableTransformer`],
//!   [`ChunkerTransformer`], [`Persist`] and [`NodeCache`], plus [`indexing::Node`] and
//!   [`indexing::IndexingStream`].
//! - Query contracts: [`TransformQuery`], [`Retrieve`], [`TransformResponse`], [`Answer`] and
//!   [`EvaluateQuery`], plus the query states and search strategies in [`querying`].
//! - Model contracts: [`SimplePrompt`], [`EmbeddingModel`], [`SparseEmbeddingModel`] and
//!   [`ChatCompletion`], with chat messages and tool schemas in [`chat_completion`].
//! - Agent contracts: [`AgentContext`], [`MessageHistory`], [`ToolExecutor`], and prompt templating
//!   in [`prompt`].
//!
//! Guides: [indexing](https://swiftide.rs/indexing/), [querying](https://swiftide.rs/querying/),
//! [models](https://swiftide.rs/models/) and [agents](https://swiftide.rs/agents/).
// show feature flags in the generated documentation
// https://doc.rust-lang.org/rustdoc/unstable-features.html#extensions-to-the-doc-attribute
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, doc(auto_cfg))]
#![doc(html_logo_url = "https://github.com/bosun-ai/swiftide/raw/master/images/logo.png")]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

pub mod agent_traits;
pub mod chat_completion;
pub mod command_output;
pub mod indexing_decorators;
mod indexing_defaults;
mod indexing_stream;
pub mod indexing_traits;
mod node;
mod query;
mod query_stream;
pub mod query_traits;
mod search_strategies;
mod stream_backoff;
pub mod token_estimation;
pub mod tool_executor;
pub mod type_aliases;

pub mod document;
pub mod prompt;
pub use type_aliases::*;

mod metadata;
mod query_evaluation;

/// All traits are available from the root
pub use crate::agent_traits::{AgentContext, MessageHistory, ToolFeedback};
pub use crate::chat_completion::traits::*;
pub use crate::command_output::*;
pub use crate::indexing_traits::*;
pub use crate::query_traits::*;
pub use crate::token_estimation::*;
pub use crate::tool_executor::*;

// Decorators are available from the root
pub use crate::indexing_decorators::*;

pub mod indexing {
    pub use crate::indexing_decorators::*;
    pub use crate::indexing_defaults::*;
    pub use crate::indexing_stream::IndexingStream;
    pub use crate::indexing_traits::*;
    pub use crate::metadata::*;
    pub use crate::node::*;
}

pub mod querying {
    pub use crate::document::*;
    pub use crate::query::*;
    pub use crate::query_evaluation::*;
    pub use crate::query_stream::*;
    pub use crate::query_traits::*;
    pub mod search_strategies {
        pub use crate::search_strategies::*;
    }
}

/// Re-export of commonly used dependencies.
pub mod prelude;

#[cfg(feature = "test-utils")]
pub mod test_utils;

pub mod util;

#[cfg(feature = "metrics")]
pub mod metrics;

/// Pipeline statistics collection for monitoring and observability
pub mod statistics;
