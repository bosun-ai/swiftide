use anyhow::Result;
use futures_util::{StreamExt as _, TryStreamExt as _};
use std::{
    borrow::{Borrow, Cow},
    sync::Arc,
};
use tokio::sync::mpsc::Sender;

use tracing::Instrument as _;

use crate::{
    search_strategy::SearchStrategy,
    traits::{self, Retrieve, TransformQuery, TransformResponse},
};

use super::{query_stream::QueryStream, Query};

/// TODO: Playing around with strategy
/// Marker trait is _very_ loose
///
/// Probably better to have a full trait, and then implement that for
/// individual structs. Enums are not types.
pub struct Pipeline<SEARCH_STRATEGY: traits::SearchStrategy = SearchStrategy> {
    search_strategy: SEARCH_STRATEGY,
    stream: QueryStream,
    query_sender: Sender<Result<Query>>,
}

impl Default for Pipeline {
    fn default() -> Self {
        let stream = QueryStream::default();
        Self {
            search_strategy: Default::default(),
            query_sender: stream
                .sender
                .clone()
                .expect("Pipeline received stream without query entrypoint"),
            stream,
        }
    }
}

impl Pipeline {
    pub fn with_search_strategy(&mut self, strategy: SearchStrategy) -> &mut Pipeline {
        self.search_strategy = strategy;

        self
    }

    /// TODO: Play around with api here
    ///
    /// Try to:
    /// Enable passing by ref
    /// Make pipeline mutable, so that you don't need to do `pipeline = pipeline...`, that's just
    /// dumb
    pub fn then_transform_query<T: ToOwned<Owned = impl TransformQuery + 'static>>(
        &mut self,
        transformer: T,
    ) -> &mut Pipeline {
        let transformer = Arc::new(transformer.to_owned());
        let stream = std::mem::take(&mut self.stream);

        self.stream = stream
            .map_ok(move |query| {
                let transformer = Arc::clone(&transformer);
                let span = tracing::trace_span!("then_transform_query", query = ?query, transformer = ?transformer);

                async move { transformer.transform_query(query).await }.instrument(span)
            })
            .try_buffer_unordered(1)
            .boxed()
            .into();

        self
    }

    pub fn then_retrieve<T: ToOwned<Owned = impl Retrieve + 'static>>(
        &mut self,
        retriever: T,
    ) -> &mut Pipeline {
        let retriever = Arc::new(retriever.to_owned());
        let stream = std::mem::take(&mut self.stream);

        self.stream = stream
            .map_ok(move |query| {
                let transformer = Arc::clone(&retriever);
                let span =
                    tracing::trace_span!("then_retrieve", query = ?query, retriever = ?retriever);

                async move { transformer.retrieve(query).await }.instrument(span)
            })
            .try_buffer_unordered(1)
            .boxed()
            .into();

        self
    }

    pub fn then_transform_response<T: ToOwned<Owned = impl TransformResponse + 'static>>(
        &mut self,
        transformer: T,
    ) -> &mut Pipeline {
        let transformer = Arc::new(transformer.to_owned());
        let stream = std::mem::take(&mut self.stream);

        self.stream = stream
            .map_ok(move |query| {
                let transformer = Arc::clone(&transformer);
                let span = tracing::trace_span!("then_transform_response", query = ?query, transformer = ?transformer);

                async move { transformer.transform_response(query).await }.instrument(span)
            })
            .try_buffer_unordered(1)
            .boxed()
            .into();

        self
    }

    pub fn query(&mut self, query: impl Into<Query>) -> Result<Query> {}
}
