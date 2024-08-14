use crate::querying::{states, Query};

pub enum QueryEvaluation {
    /// Retrieve documents
    RetrieveDocuments(Query<states::Retrieved>),
    /// Answer the query
    AnswerQuery(Query<states::Answered>),
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
