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
    traits::{Answer, Retrieve, SearchStrategy, TransformQuery, TransformResponse},
};

// # Things to consider
//
// - For a simple flow this structure should work
// - However, say we do subquestion generation, federate the queries, and then
// want to rollup / rerank, a logical step might be generate more queries
// - Similarly, if we do hybrid search or multi storage search, we might end up with multiple
// datasets that would need to be filtered and consolidated

use super::{
    query_stream::QueryStream,
    states::{self},
    Query,
};

pub struct Pipeline<'a, S: SearchStrategy = SimilaritySingleEmbedding, T = states::Pending> {
    search_strategy: S,
    stream: QueryStream<'a, T>,
    query_sender: Sender<Result<Query<states::Pending>>>,
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

impl<'stream: 'static, S> Pipeline<'stream, S, states::Pending>
where
    S: SearchStrategy,
{
    pub fn then_transform_query<T: ToOwned<Owned = impl TransformQuery + 'stream>>(
        self,
        transformer: T,
    ) -> Pipeline<'stream, S, states::Pending> {
        let transformer = Arc::new(transformer.to_owned());

        let Pipeline {
            stream,
            query_sender,
            search_strategy,
        } = self;

        let new_stream = stream
            .map_ok(move |query| {
                let transformer = Arc::clone(&transformer);
                let span = tracing::trace_span!("then_transform_query", query = ?query, transformer = ?transformer);

                async move { transformer.transform_query(query).await }.instrument(span)
            })
            .try_buffer_unordered(1);

        Pipeline {
            stream: new_stream.boxed().into(),
            search_strategy,
            query_sender,
        }
    }
}

impl<'stream: 'static, S: SearchStrategy> Pipeline<'stream, S, states::Pending> {
    pub fn then_retrieve<T: ToOwned<Owned = impl Retrieve<S> + 'stream>>(
        self,
        retriever: T,
    ) -> Pipeline<'stream, S, states::Retrieved> {
        let retriever = Arc::new(retriever.to_owned());
        let Pipeline {
            stream,
            query_sender,
            search_strategy,
        } = self;

        let new_stream = stream
            .map_ok(move |query| {
                let retriever = Arc::clone(&retriever);
                let span =
                    tracing::trace_span!("then_retrieve", query = ?query, retriever = ?retriever);
                async move { retriever.retrieve(query).await }.instrument(span)
            })
            .try_buffer_unordered(1);

        Pipeline {
            stream: new_stream.boxed().into(),
            search_strategy,
            query_sender,
        }
    }
}

impl<'stream: 'static, S: SearchStrategy> Pipeline<'stream, S, states::Retrieved> {
    pub fn then_transform_response<T: ToOwned<Owned = impl TransformResponse + 'stream>>(
        self,
        transformer: T,
    ) -> Pipeline<'stream, S, states::Retrieved> {
        let transformer = Arc::new(transformer.to_owned());
        let Pipeline {
            stream,
            query_sender,
            search_strategy,
        } = self;

        let new_stream = stream
            .map_ok(move |query| {
                let transformer = Arc::clone(&transformer);
                let span = tracing::trace_span!("then_transform_response", query = ?query, transformer = ?transformer);
                async move { transformer.transform_response(query).await }.instrument(span)
            })
            .try_buffer_unordered(1);

        Pipeline {
            stream: new_stream.boxed().into(),
            search_strategy,
            query_sender,
        }
    }
}

// Now for answering
impl<'stream: 'static, S: SearchStrategy> Pipeline<'stream, S, states::Retrieved> {
    pub fn then_answer<T: ToOwned<Owned = impl Answer + 'stream>>(
        self,
        answerer: T,
    ) -> Pipeline<'stream, S, states::Answered> {
        let answerer = Arc::new(answerer.to_owned());
        let Pipeline {
            stream,
            query_sender,
            search_strategy,
        } = self;
        let new_stream = stream
            .map_ok(move |query: Query<states::Retrieved>| {
                let answerer = Arc::clone(&answerer);
                let span =
                    tracing::trace_span!("then_answer", query = ?query, answerer = ?answerer);
                async move { answerer.answer(query).await }.instrument(span)
            })
            .try_buffer_unordered(1);
        Pipeline {
            stream: new_stream.boxed().into(),
            search_strategy,
            query_sender,
        }
    }
}

impl<'a, S: SearchStrategy> Pipeline<'a, S> {
    pub fn with_search_strategy(&mut self, strategy: S) -> &mut Pipeline<'a, S> {
        self.search_strategy = strategy.into();

        self
    }

    // pub fn then_retrieve<T: ToOwned<Owned = impl Retrieve<S> + 'static>>(
    //     &mut self,
    //     retriever: T,
    // ) -> &mut Pipeline<S> {
    //     let retriever = Arc::new(retriever.to_owned());
    //     let stream = std::mem::take(&mut self.stream);
    //
    //     self.stream = stream
    //         .map_ok(move |query| {
    //             let transformer = Arc::clone(&retriever);
    //             let span =
    //                 tracing::trace_span!("then_retrieve", query = ?query, retriever = ?retriever);
    //
    //             async move { transformer.retrieve(query).await }.instrument(span)
    //         })
    //         .try_buffer_unordered(1)
    //         .boxed()
    //         .into();
    //
    //     self
    // }

    // pub fn then_transform_response<T: ToOwned<Owned = impl TransformResponse + 'static>>(
    //     &mut self,
    //     transformer: T,
    // ) -> &mut Pipeline<S> {
    //     let transformer = Arc::new(transformer.to_owned());
    //     let stream = std::mem::take(&mut self.stream);
    //
    //     self.stream = stream
    //         .map_ok(move |query| {
    //             let transformer = Arc::clone(&transformer);
    //             let span = tracing::trace_span!("then_transform_response", query = ?query, transformer = ?transformer);
    //
    //             async move { transformer.transform_response(query).await }.instrument(span)
    //         })
    //         .try_buffer_unordered(1)
    //         .boxed()
    //         .into();
    //
    //     self
    // }

    pub fn query(
        &mut self,
        query: impl Into<Query<states::Pending>>,
    ) -> Result<Query<states::Answered>> {
    }
}
