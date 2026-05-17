use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use swiftide_agents::tasks::{
    ConcurrencyModel, JoinInput, NodeId, Task, TaskError, TaskNode, TaskRunState, Transition,
};
use tokio::{
    sync::Barrier,
    time::{sleep, timeout},
};

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
#[error("{0}")]
struct Error(String);

#[derive(Clone, Default, Debug)]
struct IntNode;

#[async_trait]
impl TaskNode for IntNode {
    type Input = i32;
    type Output = i32;
    type Error = Error;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        Ok(input + 1)
    }
}

#[derive(Clone, Debug)]
struct OffsetNode(i32);

#[async_trait]
impl TaskNode for OffsetNode {
    type Input = i32;
    type Output = i32;
    type Error = Error;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        Ok(*input + self.0)
    }
}

#[derive(Clone, Debug)]
struct DelayedOffsetNode {
    offset: i32,
    delay: Duration,
}

#[async_trait]
impl TaskNode for DelayedOffsetNode {
    type Input = i32;
    type Output = i32;
    type Error = Error;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        sleep(self.delay).await;
        Ok(*input + self.offset)
    }
}

#[derive(Clone, Debug)]
struct CompletingDelayNode {
    completed: Arc<AtomicBool>,
    delay: Duration,
}

#[async_trait]
impl TaskNode for CompletingDelayNode {
    type Input = i32;
    type Output = i32;
    type Error = Error;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        sleep(self.delay).await;
        self.completed.store(true, Ordering::SeqCst);
        Ok(*input)
    }
}

#[derive(Clone, Default, Debug)]
struct SumJoinNode;

#[async_trait]
impl TaskNode for SumJoinNode {
    type Input = JoinInput;
    type Output = i32;
    type Error = Error;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        Ok(input.iter::<i32>().copied().sum())
    }
}

#[derive(Clone, Default, Debug)]
struct CollectJoinNode;

#[async_trait]
impl TaskNode for CollectJoinNode {
    type Input = JoinInput;
    type Output = Vec<i32>;
    type Error = Error;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        Ok(input.iter::<i32>().copied().collect())
    }
}

#[derive(Clone, Default, Debug)]
struct PauseOnceNode;

#[async_trait]
impl TaskNode for PauseOnceNode {
    type Input = i32;
    type Output = i32;
    type Error = Error;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        Ok(*input)
    }
}

#[derive(Clone, Default, Debug)]
struct FailingNode;

#[async_trait]
impl TaskNode for FailingNode {
    type Input = i32;
    type Output = i32;
    type Error = Error;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        _input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        Err(Error("boom".into()))
    }
}

#[derive(Clone, Debug)]
struct BarrierNode {
    barrier: Arc<Barrier>,
}

#[async_trait]
impl TaskNode for BarrierNode {
    type Input = i32;
    type Output = i32;
    type Error = Error;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        self.barrier.wait().await;
        Ok(*input + 1)
    }
}

#[derive(Debug)]
struct CloneCountingNode {
    clone_count: Arc<std::sync::atomic::AtomicUsize>,
}

impl Clone for CloneCountingNode {
    fn clone(&self) -> Self {
        self.clone_count.fetch_add(1, Ordering::SeqCst);
        Self {
            clone_count: self.clone_count.clone(),
        }
    }
}

#[async_trait]
impl TaskNode for CloneCountingNode {
    type Input = i32;
    type Output = i32;
    type Error = Error;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        Ok(*input)
    }
}

#[test_log::test(tokio::test)]
async fn sequential_3_node_task_reset_works() {
    let mut task: Task<i32, i32> = Task::new();

    let node1 = task.register_node(IntNode);
    let node2 = task.register_node(IntNode);
    let node3 = task.register_node(IntNode);

    task.starts_with(node1);
    task.register_transition(node1, move |input| node2.transitions_with(input))
        .unwrap();
    task.register_transition(node2, move |input| node3.transitions_with(input))
        .unwrap();
    task.register_transition(node3, task.transitions_to_finish())
        .unwrap();

    let res = task.run(1).await.unwrap();
    assert_eq!(res, TaskRunState::Completed(4));

    task.reset();

    let rerun = task.resume().await.unwrap();
    assert_eq!(rerun, TaskRunState::Completed(4));
}

#[test_log::test(tokio::test)]
async fn running_task_does_not_clone_registered_nodes() {
    let clone_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(CloneCountingNode {
        clone_count: clone_count.clone(),
    });

    task.starts_with(start);
    task.register_transition(start, task.transitions_to_finish())
        .unwrap();

    assert_eq!(task.run(1).await.unwrap(), TaskRunState::Completed(1));
    assert_eq!(clone_count.load(Ordering::SeqCst), 0);
}

#[test_log::test(tokio::test)]
async fn fan_out_can_join_multiple_branches() {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(IntNode);
    let branch_a = task.register_node(IntNode);
    let branch_b = task.register_node(IntNode);
    let join = task.register_node(SumJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out(&branch_a, input)
            .and(&branch_b, input)
            .join_with(join.join())
    })
    .unwrap();
    task.register_transition(branch_a, join.join()).unwrap();
    task.register_transition(branch_b, join.join()).unwrap();
    task.register_transition(join, task.transitions_to_finish())
        .unwrap();

    let result = task.run(1).await.unwrap();
    assert_eq!(result, TaskRunState::Completed(6));
}

#[test_log::test(tokio::test)]
async fn transition_pause_pauses_task() {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(PauseOnceNode);
    task.starts_with(start);
    task.register_transition(start, move |_output| Transition::pause())
        .unwrap();

    let result = task.run(1).await.unwrap();
    assert_eq!(result, TaskRunState::Paused);
}

#[test_log::test(tokio::test)]
async fn pause_then_resume_completes() {
    let mut task: Task<i32, i32> = Task::new();

    let first_run = Arc::new(AtomicBool::new(true));
    let start = task.register_node(IntNode);
    let finish = task.register_node(IntNode);

    task.starts_with(start);
    task.register_transition(start, {
        let first_run = first_run.clone();
        move |output| {
            if first_run.swap(false, Ordering::SeqCst) {
                Transition::pause()
            } else {
                finish.transitions_with(output).into()
            }
        }
    })
    .unwrap();
    task.register_transition(finish, task.transitions_to_finish())
        .unwrap();

    assert_eq!(task.run(1).await.unwrap(), TaskRunState::Paused);
    assert_eq!(task.resume().await.unwrap(), TaskRunState::Completed(3));
}

#[test_log::test(tokio::test)]
async fn run_rejects_overwriting_active_state() {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(PauseOnceNode);
    task.starts_with(start);
    task.register_transition(start, move |_output| Transition::pause())
        .unwrap();

    assert_eq!(task.run(1).await.unwrap(), TaskRunState::Paused);
    assert!(matches!(
        task.run(2).await.unwrap_err(),
        TaskError::TaskActive
    ));
}

#[test_log::test(tokio::test)]
async fn resume_requires_resumable_state() {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(IntNode);
    task.starts_with(start);
    task.register_transition(start, task.transitions_to_finish())
        .unwrap();

    assert_eq!(task.run(1).await.unwrap(), TaskRunState::Completed(2));
    assert!(matches!(
        task.resume().await.unwrap_err(),
        TaskError::NotResumable
    ));
}

#[test_log::test(tokio::test)]
async fn no_steps_are_rejected() {
    let mut task: Task<i32, i32> = Task::new();

    assert!(matches!(task.run(1).await.unwrap_err(), TaskError::NoSteps));
}

#[test_log::test(tokio::test)]
async fn missing_transitions_are_rejected() {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(IntNode);
    let dangling = task.register_node(IntNode);

    task.starts_with(start);
    task.register_transition(start, task.transitions_to_finish())
        .unwrap();

    let error = task.run(1).await.unwrap_err();
    assert!(matches!(error, TaskError::MissingTransition(_)));
    let _ = dangling;
}

#[test_log::test(tokio::test)]
async fn node_error_fails_task() {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(FailingNode);
    task.starts_with(start);
    task.register_transition(start, task.transitions_to_finish())
        .unwrap();

    let error = task.run(1).await.unwrap_err();
    assert!(matches!(error, TaskError::NodeError(_)));
}

#[test_log::test(tokio::test)]
async fn node_error_inside_join_group_fails_task() {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(IntNode);
    let good = task.register_node(IntNode);
    let failing = task.register_node(FailingNode);
    let join = task.register_node(SumJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out(&good, input)
            .and(&failing, input)
            .join_with(join.join())
            .concurrency_model(ConcurrencyModel::Parallel)
    })
    .unwrap();
    task.register_transition(good, join.join()).unwrap();
    task.register_transition(failing, join.join()).unwrap();
    task.register_transition(join, task.transitions_to_finish())
        .unwrap();

    let error = task.run(1).await.unwrap_err();
    assert!(matches!(error, TaskError::NodeError(_)));
}

#[test_log::test(tokio::test)]
async fn transition_error_fails_non_join_branch() {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(IntNode);
    task.starts_with(start);
    task.register_transition(start, move |_output| {
        Transition::error(Error("boom".into()))
    })
    .unwrap();

    let error = task.run(1).await.unwrap_err();
    assert!(matches!(error, TaskError::NodeError(_)));
}

#[test_log::test(tokio::test)]
async fn transition_error_inside_join_group_fails_task() {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(IntNode);
    let good = task.register_node(IntNode);
    let bad = task.register_node(IntNode);
    let join = task.register_node(SumJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out(&good, input)
            .and(&bad, input)
            .join_with(join.join())
            .concurrency_model(ConcurrencyModel::Parallel)
    })
    .unwrap();
    task.register_transition(good, join.join()).unwrap();
    task.register_transition(bad, move |_output| Transition::error(Error("boom".into())))
        .unwrap();
    task.register_transition(join, task.transitions_to_finish())
        .unwrap();

    let error = task.run(1).await.unwrap_err();
    assert!(matches!(error, TaskError::NodeError(_)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn branch_error_drops_remaining_parallel_work() {
    let completed = Arc::new(AtomicBool::new(false));
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(IntNode);
    let slow = task.register_node(CompletingDelayNode {
        completed: completed.clone(),
        delay: Duration::from_millis(250),
    });
    let bad = task.register_node(IntNode);
    let join = task.register_node(SumJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out(&slow, input)
            .and(&bad, input)
            .join_with(join.join())
            .concurrency_model(ConcurrencyModel::Parallel)
    })
    .unwrap();
    task.register_transition(slow, join.join()).unwrap();
    task.register_transition(bad, move |_output| Transition::error(Error("boom".into())))
        .unwrap();
    task.register_transition(join, task.transitions_to_finish())
        .unwrap();

    let error = task.run(1).await.unwrap_err();
    assert!(matches!(error, TaskError::NodeError(_)));

    sleep(Duration::from_millis(300)).await;
    assert!(!completed.load(Ordering::SeqCst));
}

#[test_log::test(tokio::test)]
async fn fan_out_inside_active_join_group_is_rejected() {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(OffsetNode(0));
    let branch = task.register_node(IntNode);
    let nested = task.register_node(IntNode);
    let inner_join = task.register_node(SumJoinNode);
    let outer_join = task.register_node(SumJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out(&branch, input).join_with(outer_join.join())
    })
    .unwrap();
    task.register_transition(branch, move |input| {
        Transition::fan_out(&nested, input).join_with(inner_join.join())
    })
    .unwrap();
    task.register_transition(nested, inner_join.join()).unwrap();
    task.register_transition(inner_join, task.transitions_to_finish())
        .unwrap();
    task.register_transition(outer_join, task.transitions_to_finish())
        .unwrap();

    let error = task.run(1).await.unwrap_err();
    assert!(
        matches!(error, TaskError::InvalidState(message) if message.contains("cannot fan out"))
    );
}

#[test_log::test(tokio::test)]
async fn join_input_keeps_branch_creation_order() {
    let mut task: Task<i32, Vec<i32>> = Task::new();

    let start = task.register_node(OffsetNode(0));
    let first = task.register_node(OffsetNode(1));
    let second = task.register_node(OffsetNode(10));
    let join = task.register_node(CollectJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out(&first, input)
            .and(&second, input)
            .join_with(join.join())
            .concurrency_model(ConcurrencyModel::Parallel)
    })
    .unwrap();
    task.register_transition(first, join.join()).unwrap();
    task.register_transition(second, join.join()).unwrap();
    task.register_transition(join, task.transitions_to_finish())
        .unwrap();

    let result = task.run(1).await.unwrap();
    assert_eq!(result, TaskRunState::Completed(vec![2, 11]));
}

#[test_log::test(tokio::test)]
async fn fan_out_join_scope_preserves_full_fanout_join() {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(IntNode);
    let first = task.register_node(IntNode);
    let second = task.register_node(IntNode);
    let join = task.register_node(SumJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out(&first, input)
            .and(&second, input)
            .join_with(join.join())
    })
    .unwrap();
    task.register_transition(first, join.join()).unwrap();
    task.register_transition(second, join.join()).unwrap();
    task.register_transition(join, task.transitions_to_finish())
        .unwrap();

    let result = task.run(1).await.unwrap();
    assert_eq!(result, TaskRunState::Completed(6));
}

#[test_log::test(tokio::test)]
async fn fan_out_join_scope_allows_branches_to_join_after_intermediate_nodes() {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(IntNode);
    let left = task.register_node(IntNode);
    let left_tail = task.register_node(OffsetNode(10));
    let right = task.register_node(IntNode);
    let join = task.register_node(SumJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out(&left, input)
            .and(&right, input)
            .join_with(join.join())
    })
    .unwrap();
    task.register_transition(left, move |input| left_tail.transitions_with(input))
        .unwrap();
    task.register_transition(left_tail, join.join()).unwrap();
    task.register_transition(right, join.join()).unwrap();
    task.register_transition(join, task.transitions_to_finish())
        .unwrap();

    let result = task.run(1).await.unwrap();
    assert_eq!(result, TaskRunState::Completed(16));
}

#[test_log::test(tokio::test)]
async fn fan_out_join_scope_waits_for_branch_that_finishes_before_join() {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(OffsetNode(0));
    let terminal = task.register_node(IntNode);
    let slow = task.register_node(DelayedOffsetNode {
        offset: 10,
        delay: Duration::from_millis(50),
    });
    let join = task.register_node(SumJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out(&terminal, input)
            .and(&slow, input)
            .join_with(join.join())
            .concurrency_model(ConcurrencyModel::Parallel)
    })
    .unwrap();
    task.register_transition(terminal, task.transitions_to_finish())
        .unwrap();
    task.register_transition(slow, join.join()).unwrap();
    task.register_transition(join, task.transitions_to_finish())
        .unwrap();

    let result = task.run(1).await.unwrap();
    assert_eq!(result, TaskRunState::Completed(13));
}

#[test_log::test(tokio::test)]
async fn fan_out_join_scope_rejects_branch_joining_a_different_target() {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(IntNode);
    let branch = task.register_node(IntNode);
    let expected_join = task.register_node(SumJoinNode);
    let wrong_join = task.register_node(SumJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out(&branch, input).join_with(expected_join.join())
    })
    .unwrap();
    task.register_transition(branch, wrong_join.join()).unwrap();
    task.register_transition(expected_join, task.transitions_to_finish())
        .unwrap();
    task.register_transition(wrong_join, task.transitions_to_finish())
        .unwrap();

    let error = task.run(1).await.unwrap_err();
    assert!(matches!(error, TaskError::InvalidState(_)));
}

#[test_log::test(tokio::test)]
async fn register_transition_async_accepts_future_without_boxing() {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(IntNode);
    let next = task.register_node(IntNode);

    task.starts_with(start);
    task.register_transition_async(
        start,
        move |input| async move { next.transitions_with(input) },
    )
    .unwrap();
    task.register_transition(next, task.transitions_to_finish())
        .unwrap();

    let result = task.run(1).await.unwrap();
    assert_eq!(result, TaskRunState::Completed(3));
}

#[test_log::test(tokio::test)]
async fn register_transition_maps_join_payload() {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(IntNode);
    let branch = task.register_node(IntNode);
    let join = task.register_node(SumJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out(&branch, input).join_with(join.join())
    })
    .unwrap();
    task.register_transition(branch, join.join().map(|output| output * 2))
        .unwrap();
    task.register_transition(join, task.transitions_to_finish())
        .unwrap();

    let result = task.run(1).await.unwrap();
    assert_eq!(result, TaskRunState::Completed(6));
}

#[test_log::test(tokio::test)]
async fn register_transition_async_maps_join_payload() {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(IntNode);
    let branch = task.register_node(IntNode);
    let join = task.register_node(SumJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out(&branch, input).join_with(join.join())
    })
    .unwrap();
    task.register_transition_async(
        branch,
        join.join().map_async(|output| async move { output * 2 }),
    )
    .unwrap();
    task.register_transition(join, task.transitions_to_finish())
        .unwrap();

    let result = task.run(1).await.unwrap();
    assert_eq!(result, TaskRunState::Completed(6));
}

#[test]
fn conflicting_transition_registrations_are_rejected() {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(IntNode);
    let next = task.register_node(IntNode);
    let join = task.register_node(SumJoinNode);

    task.register_transition(start, move |input| next.transitions_with(input))
        .unwrap();

    let error = task.register_transition(start, join.join()).unwrap_err();
    assert!(matches!(error, TaskError::InvalidState(_)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_default_parallel_runs_fanout_branches_concurrently() {
    let barrier = Arc::new(Barrier::new(2));
    let mut task: Task<i32, i32> = Task::builder()
        .concurrency_model(ConcurrencyModel::Parallel)
        .build();

    let start = task.register_node(IntNode);
    let left = task.register_node(BarrierNode {
        barrier: barrier.clone(),
    });
    let right = task.register_node(BarrierNode { barrier });
    let join = task.register_node(SumJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out(&left, input)
            .and(&right, input)
            .join_with(join.join())
    })
    .unwrap();
    task.register_transition(left, join.join()).unwrap();
    task.register_transition(right, join.join()).unwrap();
    task.register_transition(join, task.transitions_to_finish())
        .unwrap();

    let result = timeout(Duration::from_secs(1), task.run(0))
        .await
        .expect("task default should make fan-out branches parallel")
        .expect("task run");

    assert_eq!(result, TaskRunState::Completed(4));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn join_target_concurrency_override_is_used_by_join_branch() {
    let barrier = Arc::new(Barrier::new(2));
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(IntNode);
    let first = task.register_node(IntNode);
    let first_join = task.register_node(SumJoinNode);
    let left = task.register_node(BarrierNode {
        barrier: barrier.clone(),
    });
    let right = task.register_node(BarrierNode { barrier });
    let final_join = task.register_node(SumJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out(&first, input).join_with(
            first_join
                .join()
                .concurrency_model(ConcurrencyModel::Parallel),
        )
    })
    .unwrap();
    task.register_transition(first, first_join.join()).unwrap();
    task.register_transition(first_join, move |input| {
        Transition::fan_out(&left, input)
            .and(&right, input)
            .join_with(final_join.join())
    })
    .unwrap();
    task.register_transition(left, final_join.join()).unwrap();
    task.register_transition(right, final_join.join()).unwrap();
    task.register_transition(final_join, task.transitions_to_finish())
        .unwrap();

    let result = timeout(Duration::from_secs(1), task.run(0))
        .await
        .expect("join branch concurrency override should be inherited by later fan-out")
        .expect("task run");

    assert_eq!(result, TaskRunState::Completed(6));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parallel_fanout_reaches_barrier() {
    let barrier = Arc::new(Barrier::new(2));
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(IntNode);
    let left = task.register_node(BarrierNode {
        barrier: barrier.clone(),
    });
    let right = task.register_node(BarrierNode { barrier });
    let join = task.register_node(SumJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out(&left, input)
            .and(&right, input)
            .join_with(join.join())
            .concurrency_model(ConcurrencyModel::Parallel)
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
