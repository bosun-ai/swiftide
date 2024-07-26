use std::sync::Arc;
use swiftide_core::{
    prelude::*,
    querying::{
        search_strategies::SimilaritySingleEmbedding, states, Answer, Query, QueryStream, Retrieve,
        SearchStrategy, TransformQuery, TransformResponse,
    },
};
use tokio::sync::mpsc::Sender;

pub struct Pipeline<'stream, S: SearchStrategy = SimilaritySingleEmbedding, T = states::Pending> {
    search_strategy: S,
    stream: QueryStream<'stream, T>,
    query_sender: Sender<Result<Query<states::Pending>>>,
}

impl<S: SearchStrategy> Default for Pipeline<'_, S> {
    fn default() -> Self {
        let stream = QueryStream::default();
        Self {
            search_strategy: S::default(),
            query_sender: stream
                .sender
                .clone()
                .expect("Pipeline received stream without query entrypoint"),
            stream,
        }
    }
}

impl<'a, S: SearchStrategy> Pipeline<'a, S> {
    pub fn from_search_strategy(strategy: S) -> Pipeline<'a, S> {
        Pipeline {
            search_strategy: strategy,
            ..Default::default()
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

impl<'stream: 'static, S: SearchStrategy + 'stream> Pipeline<'stream, S, states::Pending> {
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

        let strategy_for_stream = search_strategy.clone();
        let new_stream = stream
            .map_ok(move |query| {
                let search_strategy = strategy_for_stream.clone();
                let retriever = Arc::clone(&retriever);
                let span =
                    tracing::trace_span!("then_retrieve", query = ?query, retriever = ?retriever);
                async move { retriever.retrieve(&search_strategy, query).await }.instrument(span)
            })
            .try_buffer_unordered(1);

        Pipeline {
            stream: new_stream.boxed().into(),
            search_strategy: search_strategy.clone(),
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

impl<S: SearchStrategy> Pipeline<'_, S, states::Answered> {
    pub async fn query(
        mut self,
        query: impl Into<Query<states::Pending>>,
    ) -> Result<Query<states::Answered>> {
        self.query_sender.send(Ok(query.into())).await?;

        self.stream.try_next().await?.ok_or_else(|| {
            anyhow::anyhow!("Pipeline did not receive a response from the query stream")
        })
    }
}
