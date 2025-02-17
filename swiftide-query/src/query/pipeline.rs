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

use futures_util::TryFutureExt as _;
use std::sync::Arc;
use swiftide_core::{
    prelude::*,
    querying::{
        search_strategies::SimilaritySingleEmbedding, states, Answer, Query, QueryState,
        QueryStream, Retrieve, SearchStrategy, TransformQuery, TransformResponse,
    },
    EvaluateQuery,
};
use tokio::sync::mpsc::Sender;

/// The starting point of a query pipeline
pub struct Pipeline<
    'stream,
    STRATEGY: SearchStrategy = SimilaritySingleEmbedding,
    STATE: QueryState = states::Pending,
> {
    search_strategy: STRATEGY,
    stream: QueryStream<'stream, STATE>,
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

impl<'a, STRATEGY: SearchStrategy> Pipeline<'a, STRATEGY> {
    /// Create a query pipeline from a [`SearchStrategy`]
    ///
    /// # Panics
    ///
    /// Panics if the inner stream fails to build
    #[must_use]
    pub fn from_search_strategy(strategy: STRATEGY) -> Pipeline<'a, STRATEGY> {
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

impl<'stream: 'static, STRATEGY> Pipeline<'stream, STRATEGY, states::Pending>
where
    STRATEGY: SearchStrategy,
{
    /// Evaluate queries with an evaluator
    #[must_use]
    pub fn evaluate_with<T: EvaluateQuery + 'stream>(mut self, evaluator: T) -> Self {
        self.evaluator = Some(Arc::new(Box::new(evaluator)));

        self
    }

    /// Transform a query into something else, see [`crate::query_transformers`]
    #[must_use]
    pub fn then_transform_query<T: TransformQuery + 'stream>(
        self,
        transformer: T,
    ) -> Pipeline<'stream, STRATEGY, states::Pending> {
        let transformer = Arc::new(transformer);

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
                let span = tracing::info_span!("then_transform_query", query = ?query);

                tokio::spawn(
                    async move {
                        let transformed_query = transformer.transform_query(query).await?;
                        tracing::debug!(
                            transformed_query = transformed_query.current(),
                            query_transformer = transformer.name(),
                            "Transformed query"
                        );

                        Ok(transformed_query)
                    }
                    .instrument(span.or_current()),
                )
                .err_into::<anyhow::Error>()
            })
            .try_buffer_unordered(default_concurrency)
            .map(|x| x.and_then(|x| x));

        Pipeline {
            stream: new_stream.boxed().into(),
            search_strategy,
            query_sender,
            evaluator,
            default_concurrency,
        }
    }
}

impl<'stream: 'static, STRATEGY: SearchStrategy + 'stream>
    Pipeline<'stream, STRATEGY, states::Pending>
{
    /// Executes the query based on a search query with a retriever
    #[must_use]
    pub fn then_retrieve<T: ToOwned<Owned = impl Retrieve<STRATEGY> + 'stream>>(
        self,
        retriever: T,
    ) -> Pipeline<'stream, STRATEGY, states::Retrieved> {
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
                let span = tracing::info_span!("then_retrieve", query = ?query);
                let evaluator_for_stream = evaluator_for_stream.clone();

                tokio::spawn(
                    async move {
                        let result = retriever.retrieve(&search_strategy, query).await?;

                        tracing::debug!(documents = ?result.documents(), "Retrieved documents");

                        if let Some(evaluator) = evaluator_for_stream.as_ref() {
                            evaluator.evaluate(result.clone().into()).await?;
                            Ok(result)
                        } else {
                            Ok(result)
                        }
                    }
                    .instrument(span.or_current()),
                )
                .err_into::<anyhow::Error>()
            })
            .try_buffer_unordered(default_concurrency)
            .map(|x| x.and_then(|x| x));

        Pipeline {
            stream: new_stream.boxed().into(),
            search_strategy: search_strategy.clone(),
            query_sender,
            evaluator,
            default_concurrency,
        }
    }
}

impl<'stream: 'static, STRATEGY: SearchStrategy> Pipeline<'stream, STRATEGY, states::Retrieved> {
    /// Transforms a retrieved query into something else
    #[must_use]
    pub fn then_transform_response<T: TransformResponse + 'stream>(
        self,
        transformer: T,
    ) -> Pipeline<'stream, STRATEGY, states::Retrieved> {
        let transformer = Arc::new(transformer);
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
                let span = tracing::info_span!("then_transform_response", query = ?query);
                tokio::spawn(
                    async move {
                        let transformed_query = transformer.transform_response(query).await?;
                        tracing::debug!(
                            transformed_query = transformed_query.current(),
                            response_transformer = transformer.name(),
                            "Transformed response"
                        );

                        Ok(transformed_query)
                    }
                    .instrument(span.or_current()),
                )
                .err_into::<anyhow::Error>()
            })
            .try_buffer_unordered(default_concurrency)
            .map(|x| x.and_then(|x| x));

        Pipeline {
            stream: new_stream.boxed().into(),
            search_strategy,
            query_sender,
            evaluator,
            default_concurrency,
        }
    }
}

impl<'stream: 'static, STRATEGY: SearchStrategy> Pipeline<'stream, STRATEGY, states::Retrieved> {
    /// Generates an answer based on previous transformations
    #[must_use]
    pub fn then_answer<T: Answer + 'stream>(
        self,
        answerer: T,
    ) -> Pipeline<'stream, STRATEGY, states::Answered> {
        let answerer = Arc::new(answerer);
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
                let span = tracing::info_span!("then_answer", query = ?query);
                let evaluator_for_stream = evaluator_for_stream.clone();

                tokio::spawn(
                    async move {
                        tracing::debug!(answerer = answerer.name(), "Answering query");
                        let result = answerer.answer(query).await?;

                        if let Some(evaluator) = evaluator_for_stream.as_ref() {
                            evaluator.evaluate(result.clone().into()).await?;
                            Ok(result)
                        } else {
                            Ok(result)
                        }
                    }
                    .instrument(span.or_current()),
                )
                .err_into::<anyhow::Error>()
            })
            .try_buffer_unordered(default_concurrency)
            .map(|x| x.and_then(|x| x));

        Pipeline {
            stream: new_stream.boxed().into(),
            search_strategy,
            query_sender,
            evaluator,
            default_concurrency,
        }
    }
}

impl<STRATEGY: SearchStrategy> Pipeline<'_, STRATEGY, states::Answered> {
    /// Runs the pipeline with a user query, accepts `&str` as well.
    ///
    /// # Errors
    ///
    /// Errors if any of the transformations failed or no response was found
    #[tracing::instrument(skip_all, name = "query_pipeline.query")]
    pub async fn query(
        mut self,
        query: impl Into<Query<states::Pending>>,
    ) -> Result<Query<states::Answered>> {
        tracing::debug!("Sending query");
        let now = std::time::Instant::now();

        self.query_sender.send(Ok(query.into())).await?;

        let answer = self.stream.try_next().await?.ok_or_else(|| {
            anyhow::anyhow!("Pipeline did not receive a response from the query stream")
        });

        let elapsed_in_seconds = now.elapsed().as_secs();
        tracing::warn!(
            elapsed_in_seconds,
            "Answered query in {} seconds",
            elapsed_in_seconds
        );

        answer
    }

    /// Runs the pipeline with a user query, accepts `&str` as well.
    ///
    /// Does not consume the pipeline and requires a mutable reference. This allows
    /// the pipeline to be reused.
    ///
    /// # Errors
    ///
    /// Errors if any of the transformations failed or no response was found
    #[tracing::instrument(skip_all, name = "query_pipeline.query_mut")]
    pub async fn query_mut(
        &mut self,
        query: impl Into<Query<states::Pending>>,
    ) -> Result<Query<states::Answered>> {
        tracing::warn!("Sending query");
        let now = std::time::Instant::now();

        self.query_sender.send(Ok(query.into())).await?;

        let answer = self
            .stream
            .by_ref()
            .take(1)
            .try_next()
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!("Pipeline did not receive a response from the query stream")
            });

        tracing::debug!(?answer, "Received an answer");

        let elapsed_in_seconds = now.elapsed().as_secs();
        tracing::warn!(
            elapsed_in_seconds,
            "Answered query in {} seconds",
            elapsed_in_seconds
        );

        answer
    }

    /// Runs the pipeline with multiple queries
    ///
    /// # Errors
    ///
    /// Errors if any of the transformations failed, no response was found, or the stream was
    /// closed.
    #[tracing::instrument(skip_all, name = "query_pipeline.query_all")]
    pub async fn query_all(
        self,
        queries: Vec<impl Into<Query<states::Pending>> + Clone>,
    ) -> Result<Vec<Query<states::Answered>>> {
        tracing::warn!("Sending queries");
        let now = std::time::Instant::now();

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

        let elapsed_in_seconds = now.elapsed().as_secs();
        tracing::warn!(
            num_queries = queries.len(),
            elapsed_in_seconds,
            "Answered all queries in {} seconds",
            elapsed_in_seconds
        );
        Ok(results)
    }
}

#[cfg(test)]
mod test {
    use swiftide_core::{
        querying::search_strategies, MockAnswer, MockTransformQuery, MockTransformResponse,
    };

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

    #[tokio::test]
    async fn test_all_steps_should_accept_dyn_box() {
        let mut query_transformer = MockTransformQuery::new();
        query_transformer.expect_transform_query().returning(Ok);

        let mut response_transformer = MockTransformResponse::new();
        response_transformer
            .expect_transform_response()
            .returning(Ok);
        let mut answer_transformer = MockAnswer::new();
        answer_transformer
            .expect_answer()
            .returning(|query| Ok(query.answered("OK")));

        let pipeline = Pipeline::default()
            .then_transform_query(Box::new(query_transformer) as Box<dyn TransformQuery>)
            .then_retrieve(
                |_: &search_strategies::SimilaritySingleEmbedding,
                 query: Query<states::Pending>| {
                    Ok(query.retrieved_documents(vec![]))
                },
            )
            .then_transform_response(Box::new(response_transformer) as Box<dyn TransformResponse>)
            .then_answer(Box::new(answer_transformer) as Box<dyn Answer>);
        let response = pipeline.query("What").await.unwrap();
        assert_eq!(response.answer(), "OK");
    }

    #[tokio::test]
    async fn test_reuse_with_query_mut() {
        let mut pipeline = Pipeline::default()
            .then_transform_query(move |query: Query<states::Pending>| Ok(query))
            .then_retrieve(
                move |_: &search_strategies::SimilaritySingleEmbedding,
                      query: Query<states::Pending>| {
                    Ok(query.retrieved_documents(vec![]))
                },
            )
            .then_transform_response(Ok)
            .then_answer(move |query: Query<states::Retrieved>| Ok(query.answered("Ok")));

        let response = pipeline.query_mut("What").await.unwrap();
        assert_eq!(response.answer(), "Ok");
        let response = pipeline.query_mut("What").await.unwrap();
        assert_eq!(response.answer(), "Ok");
    }
}
