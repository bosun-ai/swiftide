use async_trait::async_trait;

use super::{
    errors::NodeError,
    node::{NodeArg, NodeId, TaskNode},
};

// TODO: Gnarly api, maybe use a more generic enum wrapper
// for everything?
#[derive(Clone)]
pub struct SyncClosureTaskNode<F, I, O>
where
    F: Fn(&I) -> Result<O, NodeError> + Send + Sync + Clone + 'static,
{
    pub f: F,
    _phantom: std::marker::PhantomData<(I, O)>,
}

impl<F, I, O> SyncClosureTaskNode<F, I, O>
where
    F: Fn(&I) -> Result<O, NodeError> + Send + Sync + Clone + 'static,
    I: NodeArg + Clone,
    O: NodeArg + Clone,
{
    pub fn new(f: F) -> Self {
        SyncClosureTaskNode {
            f,
            _phantom: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<F, I, O> TaskNode for SyncClosureTaskNode<F, I, O>
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
