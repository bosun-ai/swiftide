//! Ask your codebase: index source code locally, then ask questions about it.
//!
//! ```text
//! ask-your-codebase index ./my-crate
//! ask-your-codebase ask --path ./my-crate "How are errors retried?"
//! ```

mod codebase;
mod embedding;
mod index;
mod languages;
mod llm;
mod query;
mod test_code;

use std::{io::IsTerminal as _, path::PathBuf, time::Duration};

use anyhow::{Context as _, Result};
use clap::{Parser, Subcommand};
use swiftide::integrations::treesitter::SupportedLanguages;
use tracing_subscriber::EnvFilter;

use crate::{
    codebase::Codebase,
    languages::SourceLanguages,
    llm::{LlmEndpoint, OpenAiApi},
};

/// How long `index` retries a failing prompt before giving up. Indexing sends many prompts at
/// once, so rate limits are expected and worth waiting for.
const INDEX_RETRY_BUDGET: Duration = Duration::from_mins(1);
/// How long `ask` retries a failing prompt before giving up.
const ASK_RETRY_BUDGET: Duration = Duration::from_secs(30);

/// Default log filter. Swiftide logs each retried attempt at WARN and the final give-up at ERROR
/// (target `swiftide_core::indexing_decorators`). Only errors are shown, the give-up log is
/// switched off, and the final error is reported once by this program instead.
const DEFAULT_LOG_FILTER: &str =
    "error,swiftide_integrations=off,swiftide_core::indexing_decorators=off";

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Index a codebase locally and ask questions about it",
    after_help = "Environment:\n  OPENAI_API_KEY   API key (required)\n  OPENAI_BASE_URL  OpenAI-compatible API [default: https://api.openai.com/v1]"
)]
struct Cli {
    /// Model for chunk metadata (`index`) and answers (`ask`)
    #[arg(
        long,
        global = true,
        env = "OPENAI_MODEL",
        default_value = "gpt-5.4-mini"
    )]
    model: String,

    /// Send prompts to the Responses API instead of Chat Completions
    #[arg(long, global = true)]
    responses_api: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Index (or re-index) the source files under a directory
    Index {
        /// Root of the codebase
        path: PathBuf,
        /// Languages to index; files are selected by their extensions
        #[arg(long = "lang", value_delimiter = ',', default_value = "rust")]
        languages: Vec<SupportedLanguages>,
        /// Prompts sent at once; lower it for providers with tight rate limits
        #[arg(long, default_value_t = 16)]
        concurrency: usize,
    },
    /// Ask a question about an indexed codebase
    Ask {
        question: String,
        /// Root of the codebase, as given to `index`
        #[arg(long, default_value = ".")]
        path: PathBuf,
        /// Number of chunks to retrieve
        #[arg(long, default_value_t = 20)]
        top_k: u64,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(DEFAULT_LOG_FILTER)),
        )
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();

    let cli = Cli::parse();
    let api = if cli.responses_api {
        OpenAiApi::Responses
    } else {
        OpenAiApi::ChatCompletions
    };
    let endpoint = LlmEndpoint::from_env(cli.model, api)?;

    match cli.command {
        Command::Index {
            path,
            languages,
            concurrency,
        } => {
            let codebase = Codebase::at(&path)?;
            let languages = SourceLanguages::new(languages);
            println!("Indexing {languages} files in {} ...", path.display());

            let llm = endpoint.client(INDEX_RETRY_BUDGET)?;
            let report = index::index_codebase(&codebase, &languages, llm, concurrency)
                .await
                .with_context(|| format!("Indexing failed, using the {endpoint}"))?;
            println!("{report}");
        }
        Command::Ask {
            question,
            path,
            top_k,
        } => {
            let codebase = Codebase::at(&path)?;
            let llm = endpoint.client(ASK_RETRY_BUDGET)?;
            let answer = query::ask(&codebase, &question, llm, top_k)
                .await
                .with_context(|| format!("Answering failed, using the {endpoint}"))?;
            println!("{answer}");
        }
    }

    Ok(())
}
