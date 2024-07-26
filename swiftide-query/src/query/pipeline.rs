//! A query pipeline can be used to answer a user query
//!
//! The pipeline has a sequence of steps:
//!     1. Transform the query (i.e. Generating subquestions, embeddings)
//!     2. Retrieve documents from storage
//!     3. Transform these documents into a suitable context for answering
//!     4. Answering the query
//!
//! WARN: The query pipeline is in a very early stage!
//!
//! Under the hood, it uses a [`SearchStrategy`] that an implementor of [`Retrieve`] (i.e. Qdrant)
//! must implement.
//!
//! A query pipeline is lazy and only runs when query is called.
//!
//! # Example
//!
//! ```no_run
//!
//! # use anyhow::Result;
//! # use swiftide::query::{query_transformers, self, response_transformers, answers}
//!
//! # #[tokio::main]
//! # async fn main() -> Result<()> {
//! # let qdrant_url = "url";
//! # let openai_client = swiftide::integrations::openai::OpenAI::builder().build()?;
//! query::Pipeline::default()
//!     .then_transform_query(query_transformers::GenerateSubquestions::from_client(
//!         openai_client.clone(),
//!     ))
//!     .then_transform_query(query_transformers::Embed::from_client(
//!         openai_client.clone(),
//!     ))
//!     .then_retrieve(qdrant.clone())
//!     .then_transform_response(response_transformers::Summary::from_client(
//!         openai_client.clone(),
//!     ))
//!     .then_answer(answers::Simple::from_client(openai_client.clone()))
//!     .query("What is swiftide?")
//!     .await
//! # }
//! ```

use std::sync::Arc;
use swiftide_core::{
    prelude::*,
    querying::{
        search_strategies::SimilaritySingleEmbedding, states, Answer, Query, QueryStream, Retrieve,
        SearchStrategy, TransformQuery, TransformResponse,
    },
};
use tokio::sync::mpsc::Sender;

/// The starting point of a query pipeline
pub struct Pipeline<'stream, S: SearchStrategy = SimilaritySingleEmbedding, T = states::Pending> {
    search_strategy: S,
    stream: QueryStream<'stream, T>,
    query_sender: Sender<Result<Query<states::Pending>>>,
}

/// By default the [`SearchStrategy`] is [`SimilaritySingleEmbedding`], which embed the current
/// query and returns a collection of documents.
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
    /// Create a query pipeline from a [`SearchStrategy`]
    #[must_use]
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
    /// Transform a query into something else, see [`crate::query_transformers`]
    #[must_use]
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
    /// Executes the query based on a search query with a retriever
    #[must_use]
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
    /// Transforms a retrieved query into something else
    #[must_use]
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

impl<'stream: 'static, S: SearchStrategy> Pipeline<'stream, S, states::Retrieved> {
    /// Generates an answer based on previous transformations
    #[must_use]
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
    /// Runs the pipeline with a user query, accepts `&str` as well.
    ///
    /// # Errors
    ///
    /// Errors if any of the transformations failed or no response was found
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
