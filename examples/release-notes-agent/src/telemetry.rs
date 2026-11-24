//! Logging, and optionally Langfuse tracing of every completion and tool call.
use std::{
    env,
    io::{self, IsTerminal as _},
};

use anyhow::{Result, ensure};
use swiftide::langfuse::LangfuseLayer;
use tracing_subscriber::{
    EnvFilter, Layer as _,
    filter::{LevelFilter, Targets},
    layer::SubscriberExt as _,
    util::SubscriberInitExt as _,
};

/// Keeps the Langfuse layer around so pending events can be flushed before exit.
pub struct Telemetry {
    langfuse: Option<LangfuseLayer>,
}

impl Telemetry {
    // ANCHOR: langfuse
    /// Logs warnings to stderr (override with `RUST_LOG`) and, with `trace`, sends traces to
    /// Langfuse.
    pub fn init(trace: bool) -> Result<Self> {
        let logs = tracing_subscriber::fmt::layer()
            .compact()
            .with_writer(io::stderr)
            .with_ansi(io::stderr().is_terminal())
            .with_filter(
                EnvFilter::try_from_default_env()
                    // The agent logs stops and approvals as warnings, and errors it also returns;
                    // the CLI reports both itself.
                    .unwrap_or_else(|_| EnvFilter::new("warn,swiftide_agents=off")),
            );

        let langfuse = if trace {
            for key in ["LANGFUSE_PUBLIC_KEY", "LANGFUSE_SECRET_KEY"] {
                ensure!(env::var_os(key).is_some(), "--trace needs {key}");
            }
            // Reads the keys and `LANGFUSE_URL` (default http://localhost:3000) from the env.
            Some(LangfuseLayer::default())
        } else {
            None
        };

        tracing_subscriber::registry()
            .with(logs)
            .with(
                langfuse
                    .clone()
                    // Langfuse data is logged at debug level; only send Swiftide's spans, not
                    // those of every dependency.
                    .map(|layer| {
                        layer
                            .with_filter(Targets::new().with_target("swiftide", LevelFilter::DEBUG))
                    }),
            )
            .try_init()?;

        Ok(Self { langfuse })
    }
    // ANCHOR_END: langfuse

    /// Sends whatever Langfuse events are still buffered.
    pub async fn flush(&self) -> Result<()> {
        if let Some(langfuse) = &self.langfuse {
            langfuse.flush().await?;
        }
        Ok(())
    }
}
