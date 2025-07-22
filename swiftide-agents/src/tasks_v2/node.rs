use async_trait::async_trait;

pub trait NodeArg: Clone + Send + Sync + 'static {}
impl<T: Clone + Send + Sync + std::fmt::Debug + 'static> NodeArg for T {}

#[derive(Debug)]
pub struct NoopNode<Context: NodeArg, E: std::error::Error + Send + Sync> {
    _marker: std::marker::PhantomData<(Context, E)>,
}

impl<Context, E> Default for NoopNode<Context, E>
where
    Context: NodeArg,
    E: std::error::Error + Send + Sync + 'static,
{
    fn default() -> Self {
        NoopNode {
            _marker: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<Context: NodeArg, E: std::error::Error + Send + Sync + 'static> TaskNode
    for NoopNode<Context, E>
{
    type Output = ();
    type Input = Context;
    type Error = E;

    async fn evaluate(&self, _context: &Context) -> Result<Self::Output, Self::Error> {
        Ok(())
    }
}

#[async_trait]
pub trait TaskNode: Send + Sync {
    type Input: NodeArg;
    type Output: NodeArg;
    type Error: std::error::Error + Send + Sync + 'static;

    async fn evaluate(&self, input: &Self::Input) -> Result<Self::Output, Self::Error>;
}

#[derive(Debug, PartialEq, Eq)]
pub struct NodeId<T: TaskNode> {
    pub id: usize,
    _marker: std::marker::PhantomData<T>,
}

impl<T: TaskNode> NodeId<T> {
    pub fn new(id: usize, node: &T) -> Self {
        NodeId {
            id,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T: TaskNode> Clone for NodeId<T> {
    fn clone(&self) -> Self {
        NodeId {
            id: self.id,
            _marker: std::marker::PhantomData,
        }
    }
}
