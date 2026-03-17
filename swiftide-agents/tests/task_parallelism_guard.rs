use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use swiftide_agents::tasks::{
    AsyncFn, JoinInput, NodeError, SyncFn, Task, TaskRunState, Transition,
};
use tokio::sync::Barrier;
use tokio::time::timeout;

fn sync_node() -> SyncFn<impl Fn(&i32) -> Result<i32, NodeError> + Clone, i32, i32> {
    SyncFn::new(|input: &i32| Ok::<_, NodeError>(*input + 1))
}

fn barrier_node(
    barrier: Arc<Barrier>,
) -> AsyncFn<
    impl for<'a> Fn(&'a i32) -> Pin<Box<dyn Future<Output = Result<i32, NodeError>> + Send + 'a>>
    + Clone,
    i32,
    i32,
> {
    AsyncFn::new(move |input: &i32| {
        let barrier = barrier.clone();
        Box::pin(async move {
            barrier.wait().await;
            Ok::<_, NodeError>(*input + 1)
        })
    })
}

fn join_node() -> SyncFn<impl Fn(&JoinInput) -> Result<i32, NodeError> + Clone, JoinInput, i32> {
    SyncFn::new(|input: &JoinInput| {
        Ok::<_, NodeError>(input.ready_values::<i32>().into_iter().copied().sum())
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parallel_fanout_reaches_barrier() {
    let barrier = Arc::new(Barrier::new(2));
    let mut task: Task<i32, i32> = Task::builder()
        .max_parallelism(NonZeroUsize::new(2).unwrap())
        .build();

    let start = task.register_node(sync_node());
    let left = task.register_node(barrier_node(barrier.clone()));
    let right = task.register_node(barrier_node(barrier));
    let join = task.register_node(join_node());

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out([left.target_with(input), right.target_with(input)])
            .concurrency_model(swiftide_agents::tasks::ConcurrencyModel::Parallel)
    })
    .unwrap();
    task.register_transition(left, join.join()).unwrap();
    task.register_transition(right, join.join()).unwrap();
    task.register_transition(join, task.transitions_to_finish())
        .unwrap();

    let result = timeout(Duration::from_secs(1), task.run(0))
        .await
        .expect("parallel fan-out should not deadlock")
        .expect("task run");

    assert_eq!(result, TaskRunState::Completed(4));
}
