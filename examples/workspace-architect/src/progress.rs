//! Console progress: one line per workflow event, stamped with the time since start.
//!
//! The nodes report what happens; the bookkeeping (timings, tool-call counts, the parallel
//! speed-up) lives here so it does not clutter the workflow code.

use serde::Serialize;
use std::{
    fmt::Display,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

/// Characters of tool-call arguments shown with `--verbose`.
const SHOWN_ARGS: usize = 100;

#[derive(Debug, Clone)]
pub struct Progress {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    started: Instant,
    verbose: bool,
    agent_time: Mutex<Duration>,
}

impl Progress {
    pub fn new(verbose: bool) -> Self {
        Self {
            inner: Arc::new(Inner {
                started: Instant::now(),
                verbose,
                agent_time: Mutex::default(),
            }),
        }
    }

    /// Prints a workflow event.
    pub fn step(&self, stage: &str, message: impl Display) {
        println!(
            "[{:>6.1}s] {stage:<8} {message}",
            self.elapsed().as_secs_f64()
        );
    }

    /// Prints a low-level event when running with `--verbose`.
    fn detail(&self, stage: &str, message: impl Display) {
        if self.inner.verbose {
            self.step(stage, message);
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.inner.started.elapsed()
    }

    /// Starts tracking the analysis of one crate.
    pub fn analysis(&self, crate_name: &str) -> Analysis {
        self.step("analyse", format_args!("started  {crate_name}"));
        Analysis {
            progress: self.clone(),
            crate_name: crate_name.into(),
            started: Instant::now(),
            tool_calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Reports that every branch reached the join, with the speed-up from running in parallel.
    pub fn joined(&self, summaries: usize) {
        let wall_clock = self.elapsed().as_secs_f64();
        let agent_time = self.agent_time().as_secs_f64();
        self.step(
            "join",
            format_args!(
                "{summaries} summaries; {agent_time:.0}s of agent work in {wall_clock:.0}s \
                 wall-clock ({:.1}x)",
                agent_time / wall_clock
            ),
        );
    }

    fn agent_time(&self) -> Duration {
        *self.agent_time_guard()
    }

    fn add_agent_time(&self, spent: Duration) {
        *self.agent_time_guard() += spent;
    }

    fn agent_time_guard(&self) -> std::sync::MutexGuard<'_, Duration> {
        self.inner
            .agent_time
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// Progress of one crate analysis: its duration and the tool calls its agent made.
#[derive(Debug, Clone)]
pub struct Analysis {
    progress: Progress,
    crate_name: Arc<str>,
    started: Instant,
    tool_calls: Arc<AtomicUsize>,
}

impl Analysis {
    /// Counts a tool call and shows it with `--verbose`.
    pub fn tool_called(&self, tool: &str, args: &str) {
        self.tool_calls.fetch_add(1, Ordering::Relaxed);
        let args = &args[..args.floor_char_boundary(SHOWN_ARGS)];
        self.progress
            .detail("tool", format_args!("{}: {tool}({args})", self.crate_name));
    }

    /// Reports that the agent was asked to fix its answer.
    pub fn corrected(&self, problem: &impl Display) {
        self.progress.step(
            "analyse",
            format_args!("retry    {}: {problem}", self.crate_name),
        );
    }

    /// Reports the finished analysis; with `--verbose` also the report the agent returned.
    pub fn finished(&self, report: &impl Serialize) {
        if let Ok(report) = serde_json::to_string_pretty(report) {
            self.progress
                .detail("summary", format_args!("{}: {report}", self.crate_name));
        }

        let spent = self.started.elapsed();
        self.progress.add_agent_time(spent);
        self.progress.step(
            "analyse",
            format_args!(
                "finished {} in {:.1}s ({} tool calls)",
                self.crate_name,
                spent.as_secs_f64(),
                self.tool_calls.load(Ordering::Relaxed)
            ),
        );
    }
}
