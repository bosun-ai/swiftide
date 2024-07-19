use swiftide::Embedding;

#[derive(Clone, Debug, Default)]
pub struct Query {
    original: String,
    transformed: Option<String>,
    transformation_history: Vec<QueryTransformation>,
    embedding: Option<Embedding>,
    answer: Option<String>,
}

#[derive(Clone, Debug)]
pub struct QueryTransformation {
    before: String,
    after: String,
}
