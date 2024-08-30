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
    transformation_history: Vec<TransformationEvent>,

    // TODO: How would this work when doing a rollup query?
    pub embedding: Option<Embedding>,
}

impl<T: std::fmt::Debug> std::fmt::Debug for Query<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Query")
            .field("original", &self.original)
            .field("current", &self.current)
            .field("state", &self.state)
            .field("transformation_history", &self.transformation_history)
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
            transformation_history: self.transformation_history,
            embedding: self.embedding,
        }
    }

    #[allow(dead_code)]
    pub fn history(&self) -> &Vec<TransformationEvent> {
        &self.transformation_history
    }
}

impl Query<states::Pending> {
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

    /// Add retrieved documents and transition to `states::Retrieved`
    pub fn retrieved_documents(mut self, documents: Vec<Document>) -> Query<states::Retrieved> {
        self.transformation_history
            .push(TransformationEvent::Retrieved {
                before: self.current.clone(),
                after: String::new(),
                documents: documents.clone(),
            });

        let state = states::Retrieved { documents };

        self.current.clear();
        self.transition_to(state)
    }
}

impl Query<states::Retrieved> {
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

    #[derive(Debug, Default, Clone)]
    pub struct Pending;

    #[derive(Debug, Clone)]
    pub struct Retrieved {
        pub(crate) documents: Vec<Document>,
    }
    #[derive(Debug, Clone)]
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
#[derive(Clone, Debug, PartialEq)]
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
        let documents = vec!["doc1".to_string(), "doc2".to_string()];
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
        let documents = vec!["doc1".to_string(), "doc2".to_string()];
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
        let documents = vec!["doc1".to_string(), "doc2".to_string()];
        let query = query.retrieved_documents(documents);
        let query = query.answered("the answer");

        assert_eq!(query.answer(), "the answer");
    }
}
