use crate::querying::{states, Query};

/// Wraps a query for evaluation. Used by the [`crate::query_traits::EvaluateQuery`] trait.
pub enum QueryEvaluation {
    /// Retrieve documents
    RetrieveDocuments(Query<states::Retrieved>),
    /// Answer the query
    AnswerQuery(Query<states::Answered>),
}

impl std::fmt::Debug for QueryEvaluation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryEvaluation::RetrieveDocuments(query) => {
                write!(f, "RetrieveDocuments({query:?})")
            }
            QueryEvaluation::AnswerQuery(query) => write!(f, "AnswerQuery({query:?})"),
        }
    }
}

impl From<Query<states::Retrieved>> for QueryEvaluation {
    fn from(val: Query<states::Retrieved>) -> Self {
        QueryEvaluation::RetrieveDocuments(val)
    }
}

impl From<Query<states::Answered>> for QueryEvaluation {
    fn from(val: Query<states::Answered>) -> Self {
        QueryEvaluation::AnswerQuery(val)
    }
}

// TODO: must be a nicer way, maybe not needed and full encapsulation is better anyway
impl QueryEvaluation {
    pub fn retrieve_documents_query(self) -> Option<Query<states::Retrieved>> {
        if let QueryEvaluation::RetrieveDocuments(query) = self {
            Some(query)
        } else {
            None
        }
    }

    pub fn answer_query(self) -> Option<Query<states::Answered>> {
        if let QueryEvaluation::AnswerQuery(query) = self {
            Some(query)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_retrieved() {
        let query = Query::<states::Retrieved>::new(); // Assuming Query has a new() method
        let evaluation = QueryEvaluation::from(query.clone());

        match evaluation {
            QueryEvaluation::RetrieveDocuments(q) => assert_eq!(q, query),
            QueryEvaluation::AnswerQuery(_) => panic!("Unexpected QueryEvaluation variant"),
        }
    }

    #[test]
    fn test_from_answered() {
        let query = Query::<states::Answered>::new(); // Assuming Query has a new() method
        let evaluation = QueryEvaluation::from(query.clone());

        match evaluation {
            QueryEvaluation::AnswerQuery(q) => assert_eq!(q, query),
            QueryEvaluation::RetrieveDocuments(_) => panic!("Unexpected QueryEvaluation variant"),
        }
    }

    #[test]
    fn test_retrieve_documents_query() {
        let query = Query::<states::Retrieved>::new(); // Assuming Query has a new() method
        let evaluation = QueryEvaluation::RetrieveDocuments(query.clone());

        match evaluation.retrieve_documents_query() {
            Some(q) => assert_eq!(q, query),
            None => panic!("Expected a query, got None"),
        }
    }

    #[test]
    fn test_answer_query() {
        let query = Query::<states::Answered>::new(); // Assuming Query has a new() method
        let evaluation = QueryEvaluation::AnswerQuery(query.clone());

        match evaluation.answer_query() {
            Some(q) => assert_eq!(q, query),
            None => panic!("Expected a query, got None"),
        }
    }
}
