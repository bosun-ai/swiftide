//! `release-notes-agent`: drafts release notes from the git history of a repository, streams them
//! to the terminal, and writes them only after you approve.
mod agent;
mod draft;
mod git;
mod llm;
mod mcp;
mod notes;
mod progress;
mod session;
mod telemetry;
mod terminal;
mod tools;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context as _, Result};
use clap::Parser;
use swiftide::{agents::tools::local_executor::LocalExecutor, traits::ToolExecutor};

use crate::{
    agent::ReleaseTask,
    git::{Git, Revision},
    llm::{LlmEndpoint, OpenAiApi},
    session::Outcome,
    telemetry::Telemetry,
    terminal::TerminalReviewer,
};

/// Drafts release notes for the commits since the last tag, and writes them after your approval.
#[derive(Parser)]
#[command(name = "release-notes-agent", about)]
struct Cli {
    /// The git repository to write release notes for.
    #[arg(long, default_value = ".")]
    repo: PathBuf,
    /// Tag of the previous release [default: the latest tag].
    #[arg(long, value_name = "TAG")]
    since: Option<Revision>,
    /// The version being released [default: the agent proposes one].
    #[arg(long = "version", value_name = "NEXT")]
    next_version: Option<String>,
    /// File the notes are prepended to, relative to the repository root.
    #[arg(long, default_value = "RELEASE_NOTES.md")]
    output: PathBuf,
    /// Let the agent read pull requests through the GitHub MCP server (needs `npx` and a token).
    #[arg(long)]
    mcp: bool,
    /// Send traces to Langfuse (needs `LANGFUSE_PUBLIC_KEY` and `LANGFUSE_SECRET_KEY`).
    #[arg(long)]
    trace: bool,
    /// The model to draft the notes with, served by `OPENAI_BASE_URL`.
    #[arg(long, env = "OPENAI_MODEL", default_value = "gpt-5.4")]
    model: String,
    /// Use the `OpenAI` Responses API instead of chat completions.
    #[arg(long)]
    responses_api: bool,
    /// Load environment variables from this file instead of `./.env`.
    #[arg(long, value_name = "PATH")]
    env_file: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Swiftide's HTTP clients use rustls and leave choosing a crypto provider to the application.
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("a rustls crypto provider was already installed"))?;
    match Cli::parse().env_file {
        Some(path) => {
            // dotenvy's error already repeats its io cause; report it once.
            dotenvy::from_path(&path)
                .map_err(|err| anyhow::anyhow!("cannot read env file {}: {err}", path.display()))?;
        }
        None => {
            dotenvy::dotenv().ok();
        }
    }
    // Parsed again so that flags falling back to the environment (`OPENAI_MODEL`) see the file.
    let cli = Cli::parse();

    let telemetry = Telemetry::init(cli.trace)?;
    let result = run(cli).await;
    // A failed flush must not hide why the run failed.
    if let Err(err) = telemetry.flush().await {
        eprintln!("Langfuse flush failed: {err:#}");
    }
    result
}

async fn run(cli: Cli) -> Result<()> {
    // ANCHOR: executor
    let root = Git::new(&LocalExecutor::new(&cli.repo))
        .top_level()
        .await
        .with_context(|| format!("{} is not a git repository", cli.repo.display()))?;
    // Every tool the agent runs, runs with the repository as its working directory.
    let executor: Arc<dyn ToolExecutor> = Arc::new(
        LocalExecutor::builder()
            .workdir(&root)
            .default_timeout(Some(Duration::from_mins(1)))
            .build()?,
    );
    // ANCHOR_END: executor

    let endpoint = LlmEndpoint::from_env(cli.model, OpenAiApi::from_flag(cli.responses_api))?;
    let llm = endpoint.client()?;
    let task = ReleaseTask::resolve(
        Git::new(executor.as_ref()),
        cli.since,
        cli.next_version,
        cli.output,
    )
    .await?;
    // The tools write relative to the repository root, not to `--repo`; show it as a path the
    // user can open from here.
    let notes_file = root.join(&task.notes_file);
    let notes_file = std::env::current_dir()
        .ok()
        .and_then(|cwd| notes_file.strip_prefix(cwd).ok().map(Path::to_path_buf))
        .unwrap_or(notes_file);

    let github_tools = match (cli.mcp, &task.github) {
        (false, _) => None,
        (true, None) => {
            eprintln!("--mcp: origin is not a GitHub repository. Continuing without GitHub tools.");
            None
        }
        (true, Some(_)) => mcp::github_toolbox()
            .await
            .inspect_err(|err| eprintln!("--mcp: {err:#}. Continuing without GitHub tools."))
            .ok(),
    };
    println!(
        "Drafting release notes for the {} commits since {} with {endpoint}",
        task.commit_count, task.since
    );

    let mut agent = agent::build(&llm, executor, &task, github_tools)?;
    let mut reviewer = TerminalReviewer::new(&notes_file);

    match session::draft_release_notes(&mut agent, task.query().into(), &mut reviewer).await? {
        Outcome::Written { notes } => {
            println!("\n✔ {}", notes_file.display());
            if let Some(notes) = notes {
                println!("\n{notes}");
            }
        }
        Outcome::Rejected => println!("Nothing was written."),
    }
    Ok(())
}
