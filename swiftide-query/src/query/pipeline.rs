use anyhow::Result;
use futures_util::{StreamExt as _, TryStreamExt as _};
use std::{
    borrow::{Borrow, Cow},
    sync::Arc,
};
use tokio::sync::mpsc::Sender;

use tracing::Instrument as _;

use crate::{
    search_strategy::{self, SimilaritySingleEmbedding},
    traits::{Retrieve, SearchStrategyMarker, TransformQuery, TransformResponse},
};

use super::{
    query_stream::QueryStream,
    states::{self, QueryTransformed},
    Query, TransformableQuery,
};

/// TODO: Playing around with strategy
/// Marker trait is _very_ loose
///
/// Probably better to have a full trait, and then implement that for
/// individual structs. Enums are not types.
pub struct Pipeline<'a, S: SearchStrategyMarker = SimilaritySingleEmbedding, T = states::Initial> {
    search_strategy: S,
    stream: QueryStream<'a, T>,
    query_sender: Sender<Result<Query<states::Initial>>>,
}

impl Default for Pipeline<'_> {
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

impl<'a, 'b, S: SearchStrategyMarker, Q: TransformableQuery> Pipeline<'a, S, Q> {
    pub fn then_transform_query<T: ToOwned<Owned = impl TransformQuery + 'static>>(
        &mut self,
        transformer: T,
    ) -> &mut Pipeline<'b, S, QueryTransformed> {
        let transformer = Arc::new(transformer.to_owned());
        let stream = std::mem::take(&mut self.stream);

        let new_stream: QueryStream<'a, QueryTransformed>  = stream
            .map_ok(move |query| {
                let transformer = Arc::clone(&transformer);
                let span = tracing::trace_span!("then_transform_query", query = ?query, transformer = ?transformer);

                async move { transformer.transform_query(query).await }.instrument(span)
            })
            .try_buffer_unordered(1)
            .boxed()
            .into();

        self.stream = new_stream;
        self
    }
}

impl<'a, S: SearchStrategyMarker> Pipeline<'a, S> {
    pub fn with_search_strategy(&mut self, strategy: S) -> &mut Pipeline<'a, S> {
        self.search_strategy = strategy.into();

        self
    }

    pub fn then_retrieve<T: ToOwned<Owned = impl Retrieve<S> + 'static>>(
        &mut self,
        retriever: T,
    ) -> &mut Pipeline<S> {
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
    ) -> &mut Pipeline<S> {
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

    pub fn query(
        &mut self,
        query: impl Into<Query<states::Initial>>,
    ) -> Result<Query<states::Answered>> {
    }
}
