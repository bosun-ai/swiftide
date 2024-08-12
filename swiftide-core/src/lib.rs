mod indexing_defaults;
mod indexing_stream;
pub mod indexing_traits;
mod node;
mod query;
mod query_stream;
pub mod query_traits;
mod search_strategies;
pub mod type_aliases;

pub mod prompt;
pub use type_aliases::*;

mod metadata;

/// All traits are available from the root
pub use crate::indexing_traits::*;
pub use crate::query_traits::*;

pub mod indexing {
    pub use crate::indexing_defaults::*;
    pub use crate::indexing_stream::IndexingStream;
    pub use crate::indexing_traits::*;
    pub use crate::metadata::*;
    pub use crate::node::*;
}

pub mod querying {
    pub use crate::query::*;
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
