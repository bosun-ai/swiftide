//! A query is the main object going through a query pipeline
//!
//! It acts as a statemachine, with the following transitions:
//!
//! `states::Pending`: No documents have been retrieved
//! `states::Retrieved`: Documents have been retrieved
//! `states::Answered`: The query has been answered
use derive_builder::Builder;

use crate::{document::Document, util::debug_long_utf8, Embedding, SparseEmbedding};

/// A query is the main object going through a query pipeline
///
/// It acts as a statemachine, with the following transitions:
///
/// `states::Pending`: No documents have been retrieved
/// `states::Retrieved`: Documents have been retrieved
/// `states::Answered`: The query has been answered
#[derive(Clone, Default, Builder, PartialEq)]
#[builder(setter(into))]
pub struct Query<STATE: QueryState> {
    original: String,
    #[builder(default = "self.original.clone().unwrap_or_default()")]
    current: String,
    #[builder(default = STATE::default())]
    state: STATE,
    #[builder(default)]
    transformation_history: Vec<TransformationEvent>,

    // TODO: How would this work when doing a rollup query?
    #[builder(default)]
    pub embedding: Option<Embedding>,

    #[builder(default)]
    pub sparse_embedding: Option<SparseEmbedding>,

    /// Documents the query will operate on
    ///
    /// A query can retrieve multiple times, accumulating documents
    #[builder(default)]
    pub documents: Vec<Document>,
}

impl<STATE: std::fmt::Debug + QueryState> std::fmt::Debug for Query<STATE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Query")
            .field("original", &debug_long_utf8(&self.original, 100))
            .field("current", &debug_long_utf8(&self.current, 100))
            .field("state", &self.state)
            .field("transformation_history", &self.transformation_history)
            .field("embedding", &self.embedding.is_some())
            .finish()
    }
}

impl<STATE: Clone + QueryState> Query<STATE> {
    pub fn builder() -> QueryBuilder<STATE> {
        QueryBuilder::default().clone()
    }

    /// Return the query it started with
    pub fn original(&self) -> &str {
        &self.original
    }

    /// Return the current query (or after retrieval!)
    pub fn current(&self) -> &str {
        &self.current
    }

    fn transition_to<NEWSTATE: QueryState>(self, new_state: NEWSTATE) -> Query<NEWSTATE> {
        Query {
            state: new_state,
            original: self.original,
            current: self.current,
            transformation_history: self.transformation_history,
            embedding: self.embedding,
            sparse_embedding: self.sparse_embedding,
            documents: self.documents,
        }
    }

    #[allow(dead_code)]
    pub fn history(&self) -> &Vec<TransformationEvent> {
        &self.transformation_history
    }

    /// Returns the current documents that will be used as context for answer generation
    pub fn documents(&self) -> &[Document] {
        &self.documents
    }

    /// Returns the current documents as mutable
    pub fn documents_mut(&mut self) -> &mut Vec<Document> {
        &mut self.documents
    }
}

impl<STATE: Clone + CanRetrieve> Query<STATE> {
    /// Add retrieved documents and transition to `states::Retrieved`
    pub fn retrieved_documents(mut self, documents: Vec<Document>) -> Query<states::Retrieved> {
        self.documents.extend(documents.clone());
        self.transformation_history
            .push(TransformationEvent::Retrieved {
                before: self.current.clone(),
                after: String::new(),
                documents,
            });

        let state = states::Retrieved;

        self.current.clear();
        self.transition_to(state)
    }
}

impl Query<states::Pending> {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            original: query.into(),
            ..Default::default()
        }
    }

    /// Transforms the current query
    pub fn transformed_query(&mut self, new_query: impl Into<String>) {
        let new_query = new_query.into();

        self.transformation_history
            .push(TransformationEvent::Transformed {
                before: self.current.clone(),
                after: new_query.clone(),
            });

        self.current = new_query;
    }
}

impl Query<states::Retrieved> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Transforms the current response
    pub fn transformed_response(&mut self, new_response: impl Into<String>) {
        let new_response = new_response.into();

        self.transformation_history
            .push(TransformationEvent::Transformed {
                before: self.current.clone(),
                after: new_response.clone(),
            });

        self.current = new_response;
    }

    /// Transition the query to `states::Answered`
    #[must_use]
    pub fn answered(mut self, answer: impl Into<String>) -> Query<states::Answered> {
        self.current = answer.into();
        let state = states::Answered;
        self.transition_to(state)
    }
}

impl Query<states::Answered> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the answer of the query
    pub fn answer(&self) -> &str {
        &self.current
    }
}

/// Marker trait for query states
pub trait QueryState: Send + Sync + Default {}
/// Marker trait for query states that can still retrieve
pub trait CanRetrieve: QueryState {}

/// States of a query
pub mod states {
    use super::{CanRetrieve, QueryState};

    #[derive(Debug, Default, Clone, PartialEq)]
    /// The query is pending and has not been used
    pub struct Pending;

    #[derive(Debug, Default, Clone, PartialEq)]
    /// Documents have been retrieved
    pub struct Retrieved;

    #[derive(Debug, Default, Clone, PartialEq)]
    /// The query has been answered
    pub struct Answered;

    impl QueryState for Pending {}
    impl QueryState for Retrieved {}
    impl QueryState for Answered {}

    impl CanRetrieve for Pending {}
    impl CanRetrieve for Retrieved {}
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

#[derive(Clone, PartialEq)]
/// Records changes to a query
pub enum TransformationEvent {
    Transformed {
        before: String,
        after: String,
    },
    Retrieved {
        before: String,
        after: String,
        documents: Vec<Document>,
    },
}

impl std::fmt::Debug for TransformationEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransformationEvent::Transformed { before, after } => {
                write!(
                    f,
                    "Transformed: {} -> {}",
                    &debug_long_utf8(before, 100),
                    &debug_long_utf8(after, 100)
                )
            }
            TransformationEvent::Retrieved {
                before,
                after,
                documents,
            } => {
                write!(
                    f,
                    "Retrieved: {} -> {}\nDocuments: {:?}",
                    &debug_long_utf8(before, 100),
                    &debug_long_utf8(after, 100),
                    documents.len()
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_initial_state() {
        let query = Query::<states::Pending>::from("test query");
        assert_eq!(query.original(), "test query");
        assert_eq!(query.current(), "test query");
        assert_eq!(query.history().len(), 0);
    }

    #[test]
    fn test_query_transformed_query() {
        let mut query = Query::<states::Pending>::from("test query");
        query.transformed_query("new query");
        assert_eq!(query.current(), "new query");
        assert_eq!(query.history().len(), 1);
        if let TransformationEvent::Transformed { before, after } = &query.history()[0] {
            assert_eq!(before, "test query");
            assert_eq!(after, "new query");
        } else {
            panic!("Unexpected event in history");
        }
    }

    #[test]
    fn test_query_retrieved_documents() {
        let query = Query::<states::Pending>::from("test query");
        let documents: Vec<Document> = vec!["doc1".into(), "doc2".into()];
        let query = query.retrieved_documents(documents.clone());
        assert_eq!(query.documents(), &documents);
        assert_eq!(query.history().len(), 1);
        assert!(query.current().is_empty());
        if let TransformationEvent::Retrieved {
            before,
            after,
            documents: retrieved_docs,
        } = &query.history()[0]
        {
            assert_eq!(before, "test query");
            assert_eq!(after, "");
            assert_eq!(retrieved_docs, &documents);
        } else {
            panic!("Unexpected event in history");
        }
    }

    #[test]
    fn test_query_transformed_response() {
        let query = Query::<states::Pending>::from("test query");
        let documents = vec!["doc1".into(), "doc2".into()];
        let mut query = query.retrieved_documents(documents.clone());
        query.transformed_response("new response");

        assert_eq!(query.current(), "new response");
        assert_eq!(query.history().len(), 2);
        assert_eq!(query.documents(), &documents);
        assert_eq!(query.original, "test query");
        if let TransformationEvent::Transformed { before, after } = &query.history()[1] {
            assert_eq!(before, "");
            assert_eq!(after, "new response");
        } else {
            panic!("Unexpected event in history");
        }
    }

    #[test]
    fn test_query_answered() {
        let query = Query::<states::Pending>::from("test query");
        let documents = vec!["doc1".into(), "doc2".into()];
        let query = query.retrieved_documents(documents);
        let query = query.answered("the answer");

        assert_eq!(query.answer(), "the answer");
    }
}
