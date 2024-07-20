#[derive(Clone, Debug, Default)]
pub struct Query<State> {
    original: String,
    current: String,
    state: State,
    query_transformations: Vec<TransformationEvent>,
    response_transformations: Vec<TransformationEvent>,
}

pub enum 

pub mod states {
    #[derive(Debug)]
    pub struct Initial;
    #[derive(Debug)]
    pub struct QueryTransformed {
        transformation: super::TransformationEvent,
    }
    #[derive(Debug)]
    pub struct Embedded {
        embedding: swiftide::Embedding,
    }
    #[derive(Debug)]
    pub struct Retrieved {
        documents: Vec<String>,
    }
    #[derive(Debug)]
    pub struct ResponseTransformed {
        transformation: super::TransformationEvent,
    }
    #[derive(Debug)]
    pub struct Answered {
        answer: String,
    }
}

pub trait TransformableQuery: Send + Sync + std::fmt::Debug {}
impl TransformableQuery for states::Initial {}
impl TransformableQuery for states::QueryTransformed {}

pub trait RetrievableQuery {}
impl RetrievableQuery for states::Initial {}
impl RetrievableQuery for states::QueryTransformed {}
impl RetrievableQuery for states::Embedded {}

pub trait TransformableResponse {}
impl TransformableResponse for states::Retrieved {}
impl TransformableResponse for states::ResponseTransformed {}

impl<T: AsRef<str>> From<T> for Query<states::Initial> {
    fn from(original: T) -> Self {
        Self {
            original: original.as_ref().to_string(),
            current: original.as_ref().to_string(),
            state: states::Initial,
            query_transformations: Vec::new(),
            response_transformations: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TransformationEvent {
    before: String,
    after: String,
}
