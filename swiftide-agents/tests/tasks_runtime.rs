use std::{
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use swiftide_agents::tasks::{
    BranchOutcome, ConcurrencyModel, ErrorBehavior, JoinInput, NodeId, PauseBehavior, Task,
    TaskError, TaskNode, TaskRunState, Transition,
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
        Ok(input.ready_values::<i32>().into_iter().copied().sum())
    }
}

#[derive(Clone, Default, Debug)]
struct PauseJoinNode;

#[async_trait]
impl TaskNode for PauseJoinNode {
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
        Ok(input.ready_values::<i32>().into_iter().copied().sum())
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
        Ok(input.ready_values::<i32>().into_iter().copied().collect())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct JoinSnapshot {
    ready_values: Vec<i32>,
    cancelled: usize,
    failed: usize,
    paused: usize,
    pending: usize,
    late_arrivals: usize,
}

#[derive(Clone, Default, Debug)]
struct SnapshotJoinNode;

#[async_trait]
impl TaskNode for SnapshotJoinNode {
    type Input = JoinInput;
    type Output = JoinSnapshot;
    type Error = Error;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        let mut ready_values = Vec::new();
        let mut cancelled = 0;
        let mut failed = 0;
        let mut paused = 0;
        let mut pending = 0;
        let mut late_arrivals = 0;

        for branch in input.iter() {
            match &branch.outcome {
                BranchOutcome::Ready(value) => {
                    if let Some(value) = value.downcast_ref::<i32>() {
                        ready_values.push(*value);
                    }
                }
                BranchOutcome::Cancelled => cancelled += 1,
                BranchOutcome::Failed(_) => failed += 1,
                BranchOutcome::Paused => paused += 1,
                BranchOutcome::Pending => pending += 1,
                BranchOutcome::LateArrival => late_arrivals += 1,
            }
        }

        Ok(JoinSnapshot {
            ready_values,
            cancelled,
            failed,
            paused,
            pending,
            late_arrivals,
        })
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
struct TrackingNode {
    current: Arc<AtomicUsize>,
    max: Arc<AtomicUsize>,
    delay: Duration,
}

#[async_trait]
impl TaskNode for TrackingNode {
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
        let running = self.current.fetch_add(1, Ordering::SeqCst) + 1;
        let mut observed = self.max.load(Ordering::SeqCst);

        while running > observed {
            match self
                .max
                .compare_exchange(observed, running, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => break,
                Err(actual) => observed = actual,
            }
        }

        sleep(self.delay).await;
        self.current.fetch_sub(1, Ordering::SeqCst);
        Ok(*input)
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
async fn fan_out_can_join_multiple_branches() {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(IntNode);
    let branch_a = task.register_node(IntNode);
    let branch_b = task.register_node(IntNode);
    let join = task.register_node(SumJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out([branch_a.target_with(input), branch_b.target_with(input)])
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
async fn paused_branch_keeps_other_branches_running() {
    let mut task: Task<i32, i32> = Task::builder()
        .pause_behavior(PauseBehavior::DrainRunnable)
        .build();

    let start = task.register_node(IntNode);
    let active = task.register_node(IntNode);
    let paused = task.register_node(PauseOnceNode);
    let join = task.register_node(SumJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out([active.target_with(input), paused.target_with(input)])
            .concurrency_model(ConcurrencyModel::Parallel)
    })
    .unwrap();
    task.register_transition(active, join.join_at_least(1).continue_remaining())
        .unwrap();
    task.register_transition(paused, move |_output| Transition::pause())
        .unwrap();
    task.register_transition(join, task.transitions_to_finish())
        .unwrap();

    let result = task.run(1).await.unwrap();
    assert_eq!(result, TaskRunState::Completed(3));
}

#[test_log::test(tokio::test)]
async fn explicit_joiners_can_share_a_fan_out_with_normal_branches() {
    let mut task: Task<i32, i32> = Task::builder()
        .pause_behavior(PauseBehavior::DrainRunnable)
        .build();

    let start = task.register_node(IntNode);
    let joining = task.register_node(IntNode);
    let paused = task.register_node(PauseOnceNode);
    let join = task.register_node(SumJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out([joining.target_with(input), paused.target_with(input)])
            .concurrency_model(ConcurrencyModel::Parallel)
    })
    .unwrap();
    task.register_transition(joining, join.join_at_least(1).continue_remaining())
        .unwrap();
    task.register_transition(paused, move |_output| Transition::pause())
        .unwrap();
    task.register_transition(join, task.transitions_to_finish())
        .unwrap();

    let result = task.run(1).await.unwrap();
    assert_eq!(result, TaskRunState::Completed(3));
}

#[test_log::test(tokio::test)]
async fn pause_behavior_can_pause_task() {
    let mut task: Task<i32, i32> = Task::builder()
        .pause_behavior(PauseBehavior::PauseTask)
        .build();

    let start = task.register_node(PauseOnceNode);
    task.starts_with(start);
    task.register_transition(start, move |_output| Transition::pause())
        .unwrap();

    let result = task.run(1).await.unwrap();
    assert_eq!(result, TaskRunState::Paused);
}

#[test_log::test(tokio::test)]
async fn pause_then_resume_completes() {
    let mut task: Task<i32, i32> = Task::builder()
        .pause_behavior(PauseBehavior::PauseTask)
        .build();

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
async fn pause_task_preserves_in_flight_work_that_continues_after_pause_request() {
    let mut task: Task<i32, i32> = Task::builder()
        .pause_behavior(PauseBehavior::PauseTask)
        .max_parallelism(NonZeroUsize::new(3).unwrap())
        .build();

    let start = task.register_node(IntNode);
    let pausing = task.register_node(TrackingNode {
        current: Arc::new(AtomicUsize::new(0)),
        max: Arc::new(AtomicUsize::new(0)),
        delay: Duration::from_millis(5),
    });
    let advancing = task.register_node(TrackingNode {
        current: Arc::new(AtomicUsize::new(0)),
        max: Arc::new(AtomicUsize::new(0)),
        delay: Duration::from_millis(25),
    });
    let slow = task.register_node(TrackingNode {
        current: Arc::new(AtomicUsize::new(0)),
        max: Arc::new(AtomicUsize::new(0)),
        delay: Duration::from_millis(200),
    });
    let paused_after_continue = task.register_node(IntNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out([
            pausing.target_with(input),
            advancing.target_with(input),
            slow.target_with(input),
        ])
        .concurrency_model(ConcurrencyModel::Parallel)
    })
    .unwrap();
    task.register_transition(pausing, move |_output| Transition::pause())
        .unwrap();
    task.register_transition(advancing, move |output| {
        paused_after_continue.transitions_with(output)
    })
    .unwrap();
    task.register_transition(paused_after_continue, move |_output| Transition::pause())
        .unwrap();
    task.register_transition(slow, move |_output| Transition::pause())
        .unwrap();

    let result = task.run(1).await.unwrap();
    assert_eq!(result, TaskRunState::Paused);
    assert_eq!(
        task.paused_branches().len() + task.active_branches().len(),
        3
    );
}

#[test_log::test(tokio::test)]
async fn faster_finish_wins_over_slower_pause_request() {
    let mut task: Task<i32, i32> = Task::builder()
        .pause_behavior(PauseBehavior::PauseTask)
        .max_parallelism(NonZeroUsize::new(2).unwrap())
        .build();

    let start = task.register_node(OffsetNode(0));
    let fast = task.register_node(IntNode);
    let slow = task.register_node(DelayedOffsetNode {
        offset: 10,
        delay: Duration::from_millis(150),
    });

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out([fast.target_with(input), slow.target_with(input)])
            .concurrency_model(ConcurrencyModel::Parallel)
    })
    .unwrap();
    task.register_transition(fast, task.transitions_to_finish())
        .unwrap();
    task.register_transition(slow, move |_output| Transition::pause())
        .unwrap();

    let result = task.run(1).await.unwrap();
    assert_eq!(result, TaskRunState::Completed(2));
    assert!(task.paused_branches().is_empty());
    assert!(task.active_branches().is_empty());
}

#[test_log::test(tokio::test)]
async fn faster_pause_preserves_slower_finish_work_for_resume() {
    let mut task: Task<i32, i32> = Task::builder()
        .pause_behavior(PauseBehavior::PauseTask)
        .max_parallelism(NonZeroUsize::new(2).unwrap())
        .build();

    let start = task.register_node(OffsetNode(0));
    let pausing = task.register_node(PauseOnceNode);
    let slow = task.register_node(DelayedOffsetNode {
        offset: 0,
        delay: Duration::from_millis(150),
    });
    let finish = task.register_node(IntNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out([pausing.target_with(input), slow.target_with(input)])
            .concurrency_model(ConcurrencyModel::Parallel)
    })
    .unwrap();
    task.register_transition(pausing, move |_output| Transition::pause())
        .unwrap();
    task.register_transition(slow, move |output| finish.transitions_with(output))
        .unwrap();
    task.register_transition(finish, task.transitions_to_finish())
        .unwrap();

    let result = task.run(1).await.unwrap();
    assert_eq!(result, TaskRunState::Paused);
    assert_eq!(
        task.paused_branches().len() + task.active_branches().len(),
        2
    );
}

#[test_log::test(tokio::test)]
async fn run_rejects_overwriting_active_state() {
    let mut task: Task<i32, i32> = Task::builder()
        .pause_behavior(PauseBehavior::PauseTask)
        .build();

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
async fn task_without_finish_is_incomplete() {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(IntNode);
    task.starts_with(start);
    task.register_transition(start, move |_output| Transition::fan_out(Vec::new()))
        .unwrap();

    assert!(matches!(
        task.run(1).await.unwrap_err(),
        TaskError::Incomplete
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
async fn error_behavior_can_fail_task() {
    let mut task: Task<i32, i32> = Task::builder()
        .error_behavior(ErrorBehavior::FailTask)
        .build();

    let start = task.register_node(FailingNode);
    task.starts_with(start);
    task.register_transition(start, task.transitions_to_finish())
        .unwrap();

    let error = task.run(1).await.unwrap_err();
    assert!(matches!(error, TaskError::NodeError(_)));
}

#[test_log::test(tokio::test)]
async fn local_node_failures_inside_join_groups_do_not_fail_the_task() {
    let mut task: Task<i32, i32> = Task::builder()
        .error_behavior(ErrorBehavior::Local)
        .max_parallelism(NonZeroUsize::new(2).unwrap())
        .build();

    let start = task.register_node(IntNode);
    let good = task.register_node(IntNode);
    let failing = task.register_node(FailingNode);
    let join = task.register_node(SumJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out([good.target_with(input), failing.target_with(input)])
            .concurrency_model(ConcurrencyModel::Parallel)
    })
    .unwrap();
    task.register_transition(good, join.join()).unwrap();
    task.register_transition(failing, join.join()).unwrap();
    task.register_transition(join, task.transitions_to_finish())
        .unwrap();

    let result = task.run(1).await.unwrap();
    assert_eq!(result, TaskRunState::Completed(3));
}

#[test_log::test(tokio::test)]
async fn transition_error_fails_non_join_branch() {
    let mut task: Task<i32, i32> = Task::builder().error_behavior(ErrorBehavior::Local).build();

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
async fn join_input_keeps_branch_creation_order() {
    let mut task: Task<i32, Vec<i32>> = Task::new();

    let start = task.register_node(OffsetNode(0));
    let first = task.register_node(OffsetNode(1));
    let second = task.register_node(OffsetNode(10));
    let join = task.register_node(CollectJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out([first.target_with(input), second.target_with(input)])
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
async fn cancel_remaining_reports_cancelled_leftovers_in_join_input() {
    let mut task: Task<i32, JoinSnapshot> = Task::builder()
        .max_parallelism(NonZeroUsize::new(2).unwrap())
        .build();

    let start = task.register_node(OffsetNode(0));
    let fast = task.register_node(IntNode);
    let slow = task.register_node(DelayedOffsetNode {
        offset: 10,
        delay: Duration::from_millis(200),
    });
    let queued = task.register_node(OffsetNode(100));
    let join = task.register_node(SnapshotJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out([
            fast.target_with(input),
            slow.target_with(input),
            queued.target_with(input),
        ])
        .concurrency_model(ConcurrencyModel::Parallel)
    })
    .unwrap();
    task.register_transition(fast, join.join_at_least(1).cancel_remaining())
        .unwrap();
    task.register_transition(slow, join.join_at_least(1).cancel_remaining())
        .unwrap();
    task.register_transition(queued, join.join_at_least(1).cancel_remaining())
        .unwrap();
    task.register_transition(join, task.transitions_to_finish())
        .unwrap();

    let snapshot = task.run(1).await.unwrap();
    assert_eq!(
        snapshot,
        TaskRunState::Completed(JoinSnapshot {
            ready_values: vec![2],
            cancelled: 2,
            failed: 0,
            paused: 0,
            pending: 0,
            late_arrivals: 0,
        })
    );
}

#[test_log::test(tokio::test)]
async fn all_fanout_branches_scope_preserves_full_fanout_join() {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(IntNode);
    let first = task.register_node(IntNode);
    let second = task.register_node(IntNode);
    let join = task.register_node(SumJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out([first.target_with(input), second.target_with(input)])
    })
    .unwrap();
    task.register_transition(first, join.join().all_fanout_branches())
        .unwrap();
    task.register_transition(second, join.join().all_fanout_branches())
        .unwrap();
    task.register_transition(join, task.transitions_to_finish())
        .unwrap();

    let result = task.run(1).await.unwrap();
    assert_eq!(result, TaskRunState::Completed(6));
}

#[test_log::test(tokio::test)]
async fn all_fanout_branches_scope_rejects_mixed_fan_outs() {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(IntNode);
    let joining = task.register_node(IntNode);
    let normal = task.register_node(IntNode);
    let join = task.register_node(SumJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out([joining.target_with(input), normal.target_with(input)])
    })
    .unwrap();
    task.register_transition(joining, join.join().all_fanout_branches())
        .unwrap();
    task.register_transition(normal, task.transitions_to_finish())
        .unwrap();
    task.register_transition(join, task.transitions_to_finish())
        .unwrap();

    let error = task.run(1).await.unwrap_err();
    assert!(matches!(error, TaskError::InvalidState(_)));
}

#[test_log::test(tokio::test)]
async fn join_pause_behavior_override_can_pause_task_before_other_runnable_work() {
    let mut task: Task<i32, i32> = Task::builder()
        .pause_behavior(PauseBehavior::DrainRunnable)
        .max_parallelism(NonZeroUsize::new(3).unwrap())
        .build();

    let start = task.register_node(OffsetNode(0));
    let left = task.register_node(IntNode);
    let right = task.register_node(IntNode);
    let slow_normal = task.register_node(DelayedOffsetNode {
        offset: 50,
        delay: Duration::from_millis(250),
    });
    let finish = task.register_node(IntNode);
    let join = task.register_node(PauseJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out([
            left.target_with(input),
            right.target_with(input),
            slow_normal.target_with(input),
        ])
        .concurrency_model(ConcurrencyModel::Parallel)
    })
    .unwrap();
    task.register_transition(left, join.join().pause_behavior(PauseBehavior::PauseTask))
        .unwrap();
    task.register_transition(right, join.join().pause_behavior(PauseBehavior::PauseTask))
        .unwrap();
    task.register_transition(slow_normal, move |output| finish.transitions_with(output))
        .unwrap();
    task.register_transition(finish, task.transitions_to_finish())
        .unwrap();
    task.register_transition(join, move |_output| Transition::pause())
        .unwrap();

    let result = task.run(1).await.unwrap();
    assert_eq!(result, TaskRunState::Paused);
    assert_eq!(
        task.paused_branches().len() + task.active_branches().len(),
        2
    );
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
        Transition::fan_out([branch.target_with(input)])
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
        Transition::fan_out([branch.target_with(input)])
    })
    .unwrap();
    task.register_transition_async(
        branch,
        join.join_at_least(1)
            .continue_remaining()
            .map_async(|output| async move { output * 2 }),
    )
    .unwrap();
    task.register_transition(join, task.transitions_to_finish())
        .unwrap();

    let result = task.run(1).await.unwrap();
    assert_eq!(result, TaskRunState::Completed(6));
}

#[test_log::test(tokio::test)]
async fn max_parallelism_is_enforced() {
    let current = Arc::new(AtomicUsize::new(0));
    let max = Arc::new(AtomicUsize::new(0));
    let tracking = TrackingNode {
        current: current.clone(),
        max: max.clone(),
        delay: Duration::from_millis(25),
    };

    let mut task: Task<i32, i32> = Task::builder()
        .max_parallelism(NonZeroUsize::new(2).unwrap())
        .build();

    let start = task.register_node(OffsetNode(0));
    let first = task.register_node(tracking.clone());
    let second = task.register_node(tracking.clone());
    let third = task.register_node(tracking);
    let join = task.register_node(SumJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out([
            first.target_with(input),
            second.target_with(input),
            third.target_with(input),
        ])
        .concurrency_model(ConcurrencyModel::Parallel)
    })
    .unwrap();
    task.register_transition(first, join.join()).unwrap();
    task.register_transition(second, join.join()).unwrap();
    task.register_transition(third, join.join()).unwrap();
    task.register_transition(join, task.transitions_to_finish())
        .unwrap();

    let result = task.run(2).await.unwrap();
    assert_eq!(result, TaskRunState::Completed(6));
    assert!(max.load(Ordering::SeqCst) <= 2);
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
async fn parallel_fanout_reaches_barrier() {
    let barrier = Arc::new(Barrier::new(2));
    let mut task: Task<i32, i32> = Task::builder()
        .max_parallelism(NonZeroUsize::new(2).unwrap())
        .build();

    let start = task.register_node(IntNode);
    let left = task.register_node(BarrierNode {
        barrier: barrier.clone(),
    });
    let right = task.register_node(BarrierNode { barrier });
    let join = task.register_node(SumJoinNode);

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out([left.target_with(input), right.target_with(input)])
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
