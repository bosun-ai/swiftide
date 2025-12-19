use std::pin::Pin;

use async_trait::async_trait;

use super::{
    errors::NodeError,
    node::{NodeArg, NodeId, TaskNode},
};

#[derive(Clone)]
pub struct SyncFn<F, I, O>
where
    F: Fn(&I) -> Result<O, NodeError> + Send + Sync + Clone + 'static,
{
    pub f: F,
    _phantom: std::marker::PhantomData<(I, O)>,
}

#[derive(Clone)]
pub struct AsyncFn<F, I, O>
where
    F: for<'a> Fn(&'a I) -> Pin<Box<dyn Future<Output = Result<O, NodeError>> + Send + 'a>>
        + Send
        + Sync
        + Clone
        + 'static,
{
    pub f: F,
    _phantom: std::marker::PhantomData<(I, O)>,
}

impl<F, I, O> SyncFn<F, I, O>
where
    F: Fn(&I) -> Result<O, NodeError> + Send + Sync + Clone + 'static,
    I: NodeArg + Clone,
    O: NodeArg + Clone,
{
    pub fn new(f: F) -> Self {
        SyncFn {
            f,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<F, I, O> AsyncFn<F, I, O>
where
    F: for<'a> Fn(&'a I) -> Pin<Box<dyn Future<Output = Result<O, NodeError>> + Send + 'a>>
        + Send
        + Sync
        + Clone
        + 'static,
    I: NodeArg + Clone,
    O: NodeArg + Clone,
{
    pub fn new(f: F) -> Self {
        AsyncFn {
            f,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<F> From<F> for SyncFn<F, (), ()>
where
    F: Fn(&()) -> Result<(), NodeError> + Send + Sync + Clone + 'static,
{
    fn from(f: F) -> Self {
        SyncFn::new(f)
    }
}

impl<F> From<F> for AsyncFn<F, (), ()>
where
    F: for<'a> Fn(&'a ()) -> Pin<Box<dyn Future<Output = Result<(), NodeError>> + Send + 'a>>
        + Send
        + Sync
        + Clone
        + 'static,
{
    fn from(f: F) -> Self {
        AsyncFn::new(f)
    }
}

#[async_trait]
impl<F, I, O> TaskNode for SyncFn<F, I, O>
where
    F: Fn(&I) -> Result<O, NodeError> + Clone + Send + Sync + 'static,
    I: NodeArg + Clone,
    O: NodeArg + Clone,
{
    type Input = I;
    type Output = O;
    type Error = NodeError;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        (self.f)(input)
    }
}

#[async_trait]
impl<F, I, O> TaskNode for AsyncFn<F, I, O>
where
    F: for<'a> Fn(&'a I) -> Pin<Box<dyn Future<Output = Result<O, NodeError>> + Send + 'a>>
        + Clone
        + Send
        + Sync
        + 'static,
    I: NodeArg + Clone,
    O: NodeArg + Clone,
{
    type Input = I;
    type Output = O;
    type Error = NodeError;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        (self.f)(input).await
    }
}
