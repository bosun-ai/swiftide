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

use std::sync::Arc;
use swiftide_core::{
    prelude::*,
    querying::{
        search_strategies::SimilaritySingleEmbedding, states, Answer, Query, QueryStream, Retrieve,
        SearchStrategy, TransformQuery, TransformResponse,
    },
    EvaluateQuery,
};
use tokio::sync::mpsc::Sender;

/// The starting point of a query pipeline
pub struct Pipeline<'stream, S: SearchStrategy = SimilaritySingleEmbedding, T = states::Pending> {
    search_strategy: S,
    stream: QueryStream<'stream, T>,
    query_sender: Sender<Result<Query<states::Pending>>>,
    evaluator: Option<Arc<Box<dyn EvaluateQuery>>>,
    default_concurrency: usize,
}

/// By default the [`SearchStrategy`] is [`SimilaritySingleEmbedding`], which embed the current
/// query and returns a collection of documents.
impl Default for Pipeline<'_, SimilaritySingleEmbedding> {
    fn default() -> Self {
        let stream = QueryStream::default();
        Self {
            search_strategy: SimilaritySingleEmbedding::default(),
            query_sender: stream
                .sender
                .clone()
                .expect("Pipeline received stream without query entrypoint"),
            stream,
            evaluator: None,
            default_concurrency: num_cpus::get(),
        }
    }
}

impl<'a, S: SearchStrategy> Pipeline<'a, S> {
    /// Create a query pipeline from a [`SearchStrategy`]
    ///
    /// # Panics
    ///
    /// Panics if the inner stream fails to build
    #[must_use]
    pub fn from_search_strategy(strategy: S) -> Pipeline<'a, S> {
        let stream = QueryStream::default();

        Pipeline {
            search_strategy: strategy,
            query_sender: stream
                .sender
                .clone()
                .expect("Pipeline received stream without query entrypoint"),
            stream,
            evaluator: None,
            default_concurrency: num_cpus::get(),
        }
    }
}

impl<'stream: 'static, S> Pipeline<'stream, S, states::Pending>
where
    S: SearchStrategy,
{
    /// Evaluate queries with an evaluator
    #[must_use]
    pub fn evaluate_with<T: ToOwned<Owned = impl EvaluateQuery + 'stream>>(
        mut self,
        evaluator: T,
    ) -> Self {
        self.evaluator = Some(Arc::new(Box::new(evaluator.to_owned())));

        self
    }

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
            evaluator,
            default_concurrency,
        } = self;

        let new_stream = stream
            .map_ok(move |query| {
                let transformer = Arc::clone(&transformer);
                let span = tracing::trace_span!("then_transform_query", query = ?query);

                async move { transformer.transform_query(query).await }.instrument(span)
            })
            .try_buffer_unordered(default_concurrency);

        Pipeline {
            stream: new_stream.boxed().into(),
            search_strategy,
            query_sender,
            evaluator,
            default_concurrency,
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
            evaluator,
            default_concurrency,
        } = self;

        let strategy_for_stream = search_strategy.clone();
        let evaluator_for_stream = evaluator.clone();

        let new_stream = stream
            .map_ok(move |query| {
                let search_strategy = strategy_for_stream.clone();
                let retriever = Arc::clone(&retriever);
                let span = tracing::trace_span!("then_retrieve", query = ?query);
                let evaluator_for_stream = evaluator_for_stream.clone();

                async move {
                    let result = retriever.retrieve(&search_strategy, query).await?;

                    if let Some(evaluator) = evaluator_for_stream.as_ref() {
                        evaluator.evaluate(result.clone().into()).await?;
                        Ok(result)
                    } else {
                        Ok(result)
                    }
                }
                .instrument(span)
            })
            .try_buffer_unordered(default_concurrency);

        Pipeline {
            stream: new_stream.boxed().into(),
            search_strategy: search_strategy.clone(),
            query_sender,
            evaluator,
            default_concurrency,
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
            evaluator,
            default_concurrency,
        } = self;

        let new_stream = stream
            .map_ok(move |query| {
                let transformer = Arc::clone(&transformer);
                let span = tracing::trace_span!("then_transform_response", query = ?query);
                async move { transformer.transform_response(query).await }.instrument(span)
            })
            .try_buffer_unordered(default_concurrency);

        Pipeline {
            stream: new_stream.boxed().into(),
            search_strategy,
            query_sender,
            evaluator,
            default_concurrency,
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
            evaluator,
            default_concurrency,
        } = self;
        let evaluator_for_stream = evaluator.clone();

        let new_stream = stream
            .map_ok(move |query: Query<states::Retrieved>| {
                let answerer = Arc::clone(&answerer);
                let span = tracing::trace_span!("then_answer", query = ?query);
                let evaluator_for_stream = evaluator_for_stream.clone();

                async move {
                    let result = answerer.answer(query).await?;
                    if let Some(evaluator) = evaluator_for_stream.as_ref() {
                        evaluator.evaluate(result.clone().into()).await?;
                        Ok(result)
                    } else {
                        Ok(result)
                    }
                }
                .instrument(span)
            })
            .try_buffer_unordered(default_concurrency);
        Pipeline {
            stream: new_stream.boxed().into(),
            search_strategy,
            query_sender,
            evaluator,
            default_concurrency,
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

    /// Runs the pipeline with multiple queries
    ///
    /// # Errors
    ///
    /// Errors if any of the transformations failed, no response was found, or the stream was
    /// closed.
    pub async fn query_all(
        self,
        queries: Vec<impl Into<Query<states::Pending>> + Clone>,
    ) -> Result<Vec<Query<states::Answered>>> {
        let Pipeline {
            query_sender,
            mut stream,
            ..
        } = self;

        for query in &queries {
            query_sender.send(Ok(query.clone().into())).await?;
        }
        tracing::info!("All queries sent");

        let mut results = vec![];
        while let Some(result) = stream.try_next().await? {
            tracing::debug!(?result, "Received an answer");
            results.push(result);
            if results.len() == queries.len() {
                break;
            }
        }
        Ok(results)
    }
}

#[cfg(test)]
mod test {
    use swiftide_core::querying::search_strategies;

    use super::*;

    #[tokio::test]
    async fn test_closures_in_each_step() {
        let pipeline = Pipeline::default()
            .then_transform_query(move |query: Query<states::Pending>| Ok(query))
            .then_retrieve(
                move |_: &search_strategies::SimilaritySingleEmbedding,
                      query: Query<states::Pending>| {
                    Ok(query.retrieved_documents(vec![]))
                },
            )
            .then_transform_response(Ok)
            .then_answer(move |query: Query<states::Retrieved>| Ok(query.answered("Ok")));
        let response = pipeline.query("What").await.unwrap();
        assert_eq!(response.answer(), "Ok");
    }
}
