//! `workspace-architect`: drafts an ARCHITECTURE.md for a Cargo workspace.
//!
//! The CLI only parses arguments, builds the task graph and handles the approval pause. See
//! `graph.rs` for the workflow itself.

mod analyst;
mod approval;
mod graph;
mod llm;
mod progress;
mod render;
mod summary;
mod tools;
mod workspace;

use std::{
    io::{self, BufRead as _, Write as _},
    path::PathBuf,
};

use anyhow::{Context as _, Result, bail};
use clap::Parser;
use swiftide::{
    chat_completion::ChatCompletion,
    tasks::{CurrentNodes, Task, TaskError, TaskRunOutcome},
    traits::SimplePrompt,
};
use tracing_subscriber::EnvFilter;

use crate::{
    approval::ApprovalGate,
    graph::{ArchitectSettings, architect_task},
    llm::{Endpoint, ModelArgs},
    progress::Progress,
    render::{ArchitectureDraft, WrittenDocument},
    workspace::{MemberSelection, WorkspaceRoot},
};

/// Filter for Swiftide's tracing output, in `RUST_LOG` syntax, e.g. `swiftide=debug`.
const LOG_ENV: &str = "WORKSPACE_ARCHITECT_LOG";

/// Draft an ARCHITECTURE.md for a Cargo workspace with one agent per crate.
#[derive(Debug, Parser)]
#[command(version)]
struct Args {
    /// The workspace to document: its root or any directory inside it.
    #[arg(long, default_value = ".")]
    workspace: PathBuf,

    /// Where to write the document; relative paths are resolved against the workspace root.
    #[arg(long, default_value = "ARCHITECTURE.md")]
    output: PathBuf,

    /// Write the draft without asking for approval.
    #[arg(long)]
    yes: bool,

    /// Also document unpublished members such as examples and benchmarks.
    #[arg(long)]
    all_members: bool,

    #[command(flatten)]
    model: ModelArgs,

    /// Print every tool call and the raw summary each agent returns.
    #[arg(long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Only `./.env`: searching parent directories could pick up an unrelated key from `$HOME`.
    dotenvy::from_path(".env").ok();
    // Swiftide's own tracing is opt-in and goes to stderr: the progress lines and the draft are
    // the normal output. A dedicated variable, because `RUST_LOG` is often set globally (for
    // example in a cargo config `[env]` table) and would bury the draft under debug logs.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_env(LOG_ENV).unwrap_or_else(|_| "off".into()))
        .with_writer(io::stderr)
        .init();

    let args = Args::parse();
    let endpoint = Endpoint::from_env()?;
    let root = WorkspaceRoot::locate(&args.workspace)?;
    let settings = ArchitectSettings {
        selection: if args.all_members {
            MemberSelection::All
        } else {
            MemberSelection::Published
        },
        output: root.resolve(&args.output),
        pre_approved: args.yes,
    };

    let progress = Progress::new(args.verbose);
    let llm = endpoint.client(&args.model)?;
    progress.step("model", format_args!("{} at {endpoint}", args.model));

    let written = draft_architecture(&llm, root, settings, &progress)
        .await
        .map_err(headline)?;

    match written {
        Some(written) => report(&written, &progress),
        None => println!("Draft discarded; nothing was written."),
    }
    Ok(())
}

/// Runs the task, pausing for approval unless the settings say it is pre-approved.
///
/// Returns `None` when the draft was rejected.
async fn draft_architecture<L>(
    llm: &L,
    root: WorkspaceRoot,
    settings: ArchitectSettings,
    progress: &Progress,
) -> Result<Option<WrittenDocument>, RunError>
where
    L: ChatCompletion + SimplePrompt + Clone + 'static,
{
    let mut task = architect_task(llm, settings, progress)?;

    // ANCHOR: resume
    let mut outcome = task.run(root).await?;
    loop {
        match outcome {
            TaskRunOutcome::Completed(written) => return Ok(Some(written)),
            TaskRunOutcome::Paused => {
                if !approve_paused_draft(&task)? {
                    return Ok(None);
                }
                outcome = task.resume().await?;
            }
        }
    }
    // ANCHOR_END: resume
}

/// Why a run stopped: the task graph failed, or the approval step did.
#[derive(Debug, thiserror::Error)]
enum RunError {
    #[error(transparent)]
    Task(#[from] TaskError),
    #[error(transparent)]
    Approval(#[from] anyhow::Error),
}

/// Leads with the failing node's own error. Its message names the crate or the step, which says
/// more than the task's internal node number.
fn headline(error: RunError) -> anyhow::Error {
    match error {
        RunError::Task(TaskError::NodeError(failed)) => {
            anyhow::Error::from_boxed(failed.node_error)
        }
        RunError::Task(other) => other.into(),
        RunError::Approval(other) => other,
    }
}

/// Shows the draft held by the paused approval gate and asks whether to write it.
fn approve_paused_draft(task: &Task<WorkspaceRoot, WrittenDocument>) -> Result<bool> {
    let CurrentNodes::One(gate) = task.current_nodes::<ApprovalGate>() else {
        bail!("the task paused somewhere other than the approval gate");
    };
    let draft = gate
        .pending_draft()
        .context("the approval gate paused without a draft")?;

    print_draft(&draft);
    let approved = ask("Write this ARCHITECTURE.md? [y/N] ")?;
    if approved {
        gate.approve();
    }
    Ok(approved)
}

fn print_draft(draft: &ArchitectureDraft) {
    let rule = "─".repeat(80);
    println!("\n{rule}\n{}{rule}\n", draft.as_str());
}

fn ask(question: &str) -> Result<bool> {
    print!("{question}");
    io::stdout().flush()?;

    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer)?;
    println!();
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}

fn report(written: &WrittenDocument, progress: &Progress) {
    println!(
        "Wrote {} ({} bytes) in {:.1}s.",
        written.path.display(),
        written.bytes,
        progress.elapsed().as_secs_f64()
    );
}
