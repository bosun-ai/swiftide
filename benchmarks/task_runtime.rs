use std::{collections::BTreeSet, num::NonZeroUsize, time::Duration};

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

fn join_sum_node() -> SyncFn<
    impl Fn(&swiftide::agents::tasks::JoinInput) -> Result<i32, NodeError> + Clone,
    swiftide::agents::tasks::JoinInput,
    i32,
> {
    SyncFn::new(|input: &swiftide::agents::tasks::JoinInput| {
        Ok::<_, NodeError>(input.ready_values::<i32>().into_iter().copied().sum())
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
    max_parallelism: usize,
) -> Task<i32, i32> {
    let mut task: Task<i32, i32> = Task::builder()
        .max_parallelism(NonZeroUsize::new(max_parallelism).expect("max parallelism"))
        .build();

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
        let transition =
            Transition::fan_out(fan_out_nodes.iter().map(|node| node.target_with(input)));

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

fn build_short_circuit_task(
    branches: usize,
    max_parallelism: usize,
    short_circuit: bool,
) -> Task<i32, i32> {
    let mut task: Task<i32, i32> = Task::builder()
        .max_parallelism(NonZeroUsize::new(max_parallelism).expect("max parallelism"))
        .build();

    let start = task.register_node(increment_node());
    let join = task.register_node(join_sum_node());
    let fast_branch = task.register_node_async_fn(|input: &i32| {
        Box::pin(async move { Ok::<_, NodeError>(*input + 1) })
    });
    let slow_branches = (1..branches)
        .map(|_| {
            task.register_node_async_fn(|input: &i32| {
                Box::pin(async move {
                    sleep(Duration::from_millis(1)).await;
                    Ok::<_, NodeError>(*input + 1)
                })
            })
        })
        .collect::<Vec<_>>();
    let fan_out_slow_branches = slow_branches.clone();

    task.starts_with(start);
    task.register_transition(start, move |input| {
        let targets = std::iter::once(fast_branch.target_with(input)).chain(
            fan_out_slow_branches
                .iter()
                .map(|node| node.target_with(input)),
        );

        Transition::fan_out(targets)
            .concurrency_model(swiftide::agents::tasks::ConcurrencyModel::Parallel)
    })
    .expect("fan-out transition");

    if short_circuit {
        task.register_transition(fast_branch, join.join_at_least(1).cancel_remaining())
            .expect("fast short-circuit join");
        for branch in slow_branches {
            task.register_transition(branch, join.join_at_least(1).cancel_remaining())
                .expect("slow short-circuit join");
        }
    } else {
        task.register_transition(fast_branch, join.join())
            .expect("fast join");
        for branch in slow_branches {
            task.register_transition(branch, join.join())
                .expect("slow join");
        }
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
            let max_parallelism = branches.min(runtime_threads());
            let blueprint = build_fanout_join_task(
                branches,
                Duration::from_millis(1),
                parallel,
                max_parallelism,
            );

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

fn benchmark_parallelism_cap(c: &mut Criterion, runtime: &Runtime) {
    let mut group = c.benchmark_group("tasks/parallelism-cap");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(8));

    let branches = 32_usize;
    let caps = [1_usize, 2, 4, runtime_threads()]
        .into_iter()
        .collect::<BTreeSet<_>>();

    for cap in caps {
        group.throughput(Throughput::Elements(branches as u64));
        let blueprint = build_fanout_join_task(branches, Duration::from_millis(1), true, cap);

        group.bench_with_input(BenchmarkId::new("cap", cap), &cap, |b, _| {
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

fn benchmark_join_short_circuit(c: &mut Criterion, runtime: &Runtime) {
    let mut group = c.benchmark_group("tasks/join-short-circuit");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(8));

    let branches = 16_usize;
    let max_parallelism = branches.min(runtime_threads());
    group.throughput(Throughput::Elements(branches as u64));

    for (label, short_circuit) in [("join_all", false), ("at_least_1_cancel_remaining", true)] {
        let blueprint = build_short_circuit_task(branches, max_parallelism, short_circuit);
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

    group.finish();
}

fn criterion_benchmark(c: &mut Criterion) {
    let runtime = runtime();
    benchmark_linear_run(c, &runtime);
    benchmark_fanout_modes(c, &runtime);
    benchmark_parallelism_cap(c, &runtime);
    benchmark_join_short_circuit(c, &runtime);
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
