use swiftide::Embedding;

#[derive(Clone, Debug, Default)]
pub struct Query<State> {
    original: String,
    current: String,
    state: State,
    query_transformations: Vec<TransformationEvent>,
    response_transformations: Vec<TransformationEvent>,

    // TODO: How would this work when doing a rollup query?
    embedding: Option<Embedding>,
}

pub mod states {
    #[derive(Debug, Default)]
    pub struct Pending;

    #[derive(Debug)]
    pub struct Retrieved {
        documents: Vec<String>,
    }
    #[derive(Debug)]
    pub struct Answered {
        answer: String,
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

#[derive(Clone, Debug)]
pub struct TransformationEvent {
    before: String,
    after: String,
}
