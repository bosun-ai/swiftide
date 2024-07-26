//! A query is the main object going through a query pipeline
//!
//! It acts as a statemachine, with the following transitions:
//!
//! `states::Pending`: No documents have been retrieved
//! `states::Retrieved`: Documents have been retrieved
//! `states::Answered`: The query has been answered
use crate::Embedding;

type Document = String;

#[derive(Clone, Default)]
pub struct Query<State> {
    original: String,
    current: String,
    state: State,
    query_transformations: Vec<TransformationEvent>,
    response_transformations: Vec<TransformationEvent>,

    // TODO: How would this work when doing a rollup query?
    pub embedding: Option<Embedding>,
}

impl<T: std::fmt::Debug> std::fmt::Debug for Query<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Query")
            .field("original", &self.original)
            .field("current", &self.current)
            .field("state", &self.state)
            .field("query_transformations", &self.query_transformations)
            .field("response_transformations", &self.response_transformations)
            .field("embedding", &self.embedding.is_some())
            .finish()
    }
}

impl<T> Query<T> {
    /// Return the query it started with
    pub fn original(&self) -> &str {
        &self.original
    }

    /// Return the current query (or after retrieval!)
    pub fn current(&self) -> &str {
        &self.current
    }

    fn transition_to<S>(self, new_state: S) -> Query<S> {
        Query {
            state: new_state,
            original: self.original,
            current: self.current,
            query_transformations: self.query_transformations,
            response_transformations: self.response_transformations,
            embedding: self.embedding,
        }
    }
}

impl Query<states::Pending> {
    /// Transforms the current query
    pub fn transformed_query(&mut self, new_query: impl Into<String>) {
        let new_query = new_query.into();

        self.query_transformations.push(TransformationEvent {
            before: self.current.clone(),
            after: new_query.clone(),
        });

        self.current = new_query;
    }

    /// Add retrieved documents and transition to `states::Retrieved`
    pub fn retrieved_documents(self, documents: Vec<Document>) -> Query<states::Retrieved> {
        let state = states::Retrieved { documents };

        self.transition_to(state)
    }
}

impl Query<states::Retrieved> {
    /// Transforms the current response
    pub fn transformed_response(&mut self, new_response: impl Into<String>) {
        let new_response = new_response.into();

        self.response_transformations.push(TransformationEvent {
            before: self.current.clone(),
            after: new_response.clone(),
        });

        self.current = new_response;
    }

    /// Returns the last retrieved documents
    pub fn documents(&self) -> &[Document] {
        &self.state.documents
    }

    /// Transition the query to `states::Answered`
    #[must_use]
    pub fn answered(self, answer: impl Into<String>) -> Query<states::Answered> {
        let state = states::Answered {
            answer: answer.into(),
        };
        self.transition_to(state)
    }
}

impl Query<states::Answered> {
    /// Returns the answer of the query
    pub fn answer(&self) -> &str {
        &self.state.answer
    }
}

pub mod states {
    use super::Document;

    #[derive(Debug, Default)]
    pub struct Pending;

    #[derive(Debug)]
    pub struct Retrieved {
        pub(crate) documents: Vec<Document>,
    }
    #[derive(Debug)]
    pub struct Answered {
        pub(crate) answer: String,
    }
}

impl<T: AsRef<str>> From<T> for Query<states::Pending> {
    fn from(original: T) -> Self {
        Self {
            original: original.as_ref().to_string(),
            current: original.as_ref().to_string(),
            state: states::Pending,
            ..Default::default()
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct TransformationEvent {
    before: String,
    after: String,
}
