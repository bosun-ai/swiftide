use std::time::Duration;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use swiftide::agents::tasks::{NodeError, SyncFn, Task, TaskRunState, Transition};
use tokio::runtime::{Builder, Runtime};
use tokio::time::sleep;

fn runtime_threads() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(4)
        .clamp(4, 8)
}

fn runtime() -> Runtime {
    Builder::new_multi_thread()
        .worker_threads(runtime_threads())
        .enable_all()
        .build()
        .expect("benchmark runtime")
}

fn increment_node() -> SyncFn<impl Fn(&i32) -> Result<i32, NodeError> + Clone, i32, i32> {
    SyncFn::new(|input: &i32| Ok::<_, NodeError>(*input + 1))
}

#[allow(clippy::type_complexity)]
fn join_sum_node() -> SyncFn<
    impl Fn(&swiftide::agents::tasks::JoinInput<i32>) -> Result<i32, NodeError> + Clone,
    swiftide::agents::tasks::JoinInput<i32>,
    i32,
> {
    SyncFn::new(|input: &swiftide::agents::tasks::JoinInput<i32>| {
        Ok::<_, NodeError>(input.iter().copied().sum())
    })
}

async fn complete_task(mut task: Task<i32, i32>, input: i32) -> i32 {
    match task
        .run(std::hint::black_box(input))
        .await
        .expect("task run")
    {
        TaskRunState::Completed(output) => std::hint::black_box(output),
        TaskRunState::Paused => panic!("benchmark task paused unexpectedly"),
    }
}

fn build_linear_task(depth: usize) -> Task<i32, i32> {
    let mut task: Task<i32, i32> = Task::new();
    let mut nodes = Vec::with_capacity(depth);

    for _ in 0..depth {
        nodes.push(task.register_node(increment_node()));
    }

    let first = *nodes.first().expect("linear task needs at least one node");
    task.starts_with(first);

    for window in nodes.windows(2) {
        let current = window[0];
        let next = window[1];
        task.register_transition(current, move |input| next.transitions_with(input))
            .expect("linear transition");
    }

    let last = *nodes.last().expect("linear task needs a last node");
    task.register_transition(last, task.transitions_to_finish())
        .expect("linear finish");

    task
}

fn build_fanout_join_task(
    branches: usize,
    child_delay: Duration,
    parallel: bool,
) -> Task<i32, i32> {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(increment_node());
    let join = task.register_node(join_sum_node());
    let branch_nodes = (0..branches)
        .map(|_| {
            task.register_node_async_fn(move |input: &i32| {
                Box::pin(async move {
                    sleep(child_delay).await;
                    Ok::<_, NodeError>(*input + 1)
                })
            })
        })
        .collect::<Vec<_>>();
    let fan_out_nodes = branch_nodes.clone();

    task.starts_with(start);
    task.register_transition(start, move |input| {
        let (first, remaining) = fan_out_nodes
            .split_first()
            .expect("fan-out benchmark needs at least one branch");
        let transition = remaining
            .iter()
            .fold(Transition::fan_out(first, input), |fan_out, node| {
                fan_out.and(node, input)
            })
            .join_with(join.join());

        if parallel {
            transition.concurrency_model(swiftide::agents::tasks::ConcurrencyModel::Parallel)
        } else {
            transition
        }
    })
    .expect("fan-out transition");

    for branch in branch_nodes {
        task.register_transition(branch, join.join())
            .expect("join transition");
    }

    task.register_transition(join, task.transitions_to_finish())
        .expect("finish transition");

    task
}

fn benchmark_linear_run(c: &mut Criterion, runtime: &Runtime) {
    let mut group = c.benchmark_group("tasks/linear-run");
    group.sample_size(100);

    for depth in [8_usize, 32, 128] {
        group.throughput(Throughput::Elements(depth as u64));
        let blueprint = build_linear_task(depth);
        group.bench_with_input(BenchmarkId::new("depth", depth), &depth, |b, _| {
            b.to_async(runtime).iter_batched(
                || blueprint.clone(),
                |task| async move {
                    let _ = complete_task(task, 0).await;
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

fn benchmark_fanout_modes(c: &mut Criterion, runtime: &Runtime) {
    let mut group = c.benchmark_group("tasks/fanout-sequential-vs-parallel");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(8));

    for branches in [2_usize, 8, 32] {
        group.throughput(Throughput::Elements(branches as u64));

        for (label, parallel) in [("sequential", false), ("parallel", true)] {
            let blueprint = build_fanout_join_task(branches, Duration::from_millis(1), parallel);

            group.bench_with_input(BenchmarkId::new(label, branches), &branches, |b, _| {
                b.to_async(runtime).iter_batched(
                    || blueprint.clone(),
                    |task| async move {
                        let _ = complete_task(task, 0).await;
                    },
                    BatchSize::SmallInput,
                );
            });
        }
    }

    group.finish();
}

fn criterion_benchmark(c: &mut Criterion) {
    let runtime = runtime();
    benchmark_linear_run(c, &runtime);
    benchmark_fanout_modes(c, &runtime);
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
