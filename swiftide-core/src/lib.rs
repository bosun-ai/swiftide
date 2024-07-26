mod indexing_stream;
mod indexing_traits;
mod node;
mod query;
mod query_stream;
mod query_traits;
mod search_strategy;
mod type_aliases;

pub mod prompt;
pub use type_aliases::*;

/// All traits are available from the root
pub use crate::indexing_traits::*;
pub use crate::query_traits::*;

pub mod indexing {
    pub use crate::indexing_stream::IndexingStream;
    pub use crate::indexing_traits::*;
    pub use crate::node::*;
}

pub mod querying {
    pub use crate::query::*;
    pub use crate::query_stream::*;
    pub use crate::query_traits::*;
    pub use crate::search_strategy::*;
}

/// Re-export of commonly used dependencies.
pub mod prelude;
