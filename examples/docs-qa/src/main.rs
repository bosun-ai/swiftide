//! `docs-qa`: index markdown docs into Qdrant, answer questions over them with hybrid search and
//! reranking, and evaluate the answers RAGAS-style.

mod docs;
mod eval;
mod index;
mod judge;
mod loader;
mod models;
mod query;
mod render;
mod retrieval;
mod store;
mod telemetry;

use std::{
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{Context as _, Result};
use clap::{Parser, Subcommand};

use crate::{
    eval::QuestionSet,
    judge::Judge,
    loader::{DocsLoader, DocsSource},
    models::{Embedders, LlmEndpoint},
    query::DocsQa,
    store::Collection,
    telemetry::Telemetry,
};

const DEFAULT_QUESTIONS: &str = include_str!("../questions.yaml");

/// Index markdown docs into Qdrant, answer questions over them, and evaluate the answers.
///
/// Chat models are reached through an OpenAI-compatible API: set `OPENAI_API_KEY`, and
/// `OPENAI_BASE_URL` for anything other than `OpenAI` itself.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    /// The model that answers questions, and the default for every other model
    #[arg(
        long,
        global = true,
        env = "OPENAI_MODEL",
        default_value = "gpt-5.4-mini"
    )]
    model: String,

    /// Use the Responses API instead of chat completions
    #[arg(long, global = true)]
    responses_api: bool,

    /// Send traces to Langfuse (needs `LANGFUSE_PUBLIC_KEY`, `LANGFUSE_SECRET_KEY`, and
    /// `LANGFUSE_URL` unless Langfuse runs on localhost:3000)
    #[arg(long, global = true)]
    langfuse: bool,

    /// The Qdrant collection to index into and search
    #[arg(long, global = true, default_value = "docs-qa")]
    collection: Collection,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Index markdown files, directories of markdown files, or an `llms-full.txt` (path or URL)
    Index {
        /// A markdown file, a directory of markdown files, or an `llms-full.txt` path or URL
        #[arg(required = true, value_name = "SOURCE")]
        sources: Vec<DocsSource>,

        /// Drop the collection first instead of upserting into it
        #[arg(long)]
        recreate: bool,

        /// The model that writes chunk titles and keywords (two calls per chunk; pick a cheap
        /// one). Defaults to `--model` / `OPENAI_MODEL`
        #[arg(long, value_name = "MODEL")]
        metadata_model: Option<String>,
    },
    /// Answer a question from the indexed docs
    Ask { question: String },
    /// Answer a fixed question set, score it, and export a RAGAS dataset
    Eval {
        /// YAML list of `{question, ground_truth}`; defaults to the bundled Swiftide questions
        #[arg(long, value_name = "PATH")]
        questions: Option<PathBuf>,

        /// Where to write the RAGAS dataset
        #[arg(long, default_value = "ragas.json", value_name = "PATH")]
        out: PathBuf,

        /// The model that grades answers. Defaults to `--model` / `OPENAI_MODEL`
        #[arg(long, value_name = "MODEL")]
        judge_model: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Before parsing, so `.env` can also set `OPENAI_MODEL`. Only the working directory is read;
    // `dotenvy::dotenv()` would also pick up a `.env` in any parent directory.
    if let Err(error) = dotenvy::from_path(".env")
        && !error.not_found()
    {
        return Err(error).context("Failed to load .env");
    }
    let cli = Cli::parse();

    let telemetry = Telemetry::init(cli.langfuse)?;
    let result = run(cli).await;
    if let Err(error) = telemetry.flush().await {
        eprintln!("Could not send traces to Langfuse: {error:#}");
    }
    result
}

async fn run(cli: Cli) -> Result<()> {
    let llm = LlmEndpoint::from_env(cli.responses_api)?;
    let qdrant = store::connect(&cli.collection).await?;

    match cli.command {
        Command::Index {
            sources,
            recreate,
            metadata_model,
        } => {
            if recreate {
                store::drop_collection(&qdrant, &cli.collection).await?;
            }

            let metadata_model = metadata_model.as_deref().unwrap_or(&cli.model);
            let started = Instant::now();
            index::index(
                DocsLoader::new(sources),
                &llm.retrying_model(metadata_model)?,
                &Embedders::load()?,
                &qdrant,
            )
            .await?;

            let chunks = store::chunk_count(&qdrant, &cli.collection).await?;
            println!(
                "Indexed {chunks} chunks into `{}` in {:.1?}",
                cli.collection,
                started.elapsed()
            );
        }
        Command::Ask { question } => {
            store::ensure_indexed(&qdrant, &cli.collection).await?;
            let qa = DocsQa::new(qdrant, Embedders::load()?, llm.retrying_model(&cli.model)?)?;

            let answer = qa.ask(&question).await?;
            println!("{}", render::answer(&answer));
        }
        Command::Eval {
            questions,
            out,
            judge_model,
        } => {
            store::ensure_indexed(&qdrant, &cli.collection).await?;
            let questions = match questions {
                Some(path) => QuestionSet::from_yaml(&read(&path)?)?,
                None => QuestionSet::from_yaml(DEFAULT_QUESTIONS)?,
            };
            let qa = DocsQa::new(qdrant, Embedders::load()?, llm.retrying_model(&cli.model)?)?;
            let judge_model = judge_model.as_deref().unwrap_or(&cli.model);
            let judge = Judge::new(llm.model(judge_model)?);

            let evaluation = eval::evaluate(&qa, questions, &judge).await?;

            std::fs::write(&out, &evaluation.ragas_json)
                .with_context(|| format!("Failed to write {}", out.display()))?;
            println!("{}", render::eval_table(&evaluation.rows));
            println!("RAGAS dataset written to {}", out.display());
        }
    }

    Ok(())
}

fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))
}
