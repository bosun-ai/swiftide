//! Logging to the terminal, and optionally tracing to Langfuse.

use anyhow::Result;
use swiftide::langfuse::LangfuseLayer;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{
    EnvFilter, Layer as _, layer::SubscriberExt as _, util::SubscriberInitExt as _,
};

/// Swiftide reports query timings at `warn`; keep the terminal quiet unless `RUST_LOG` says
/// otherwise.
///
/// `async_openai` logs every retried request at `warn` with the full response body, and Swiftide
/// logs a failed call at `error` right before returning it, so both stay quiet too.
const DEFAULT_LOG_FILTER: &str =
    "warn,swiftide_query=error,async_openai=error,swiftide_integrations=off";

/// Keeps the Langfuse layer around so pending events can be flushed before exiting.
pub struct Telemetry {
    langfuse: Option<LangfuseLayer>,
}

impl Telemetry {
    pub fn init(send_to_langfuse: bool) -> Result<Self> {
        if send_to_langfuse {
            ensure_langfuse_keys()?;
        }

        // ANCHOR: langfuse
        let terminal = tracing_subscriber::fmt::layer()
            .compact()
            .with_writer(std::io::stderr)
            .with_filter(log_filter());

        // Reads LANGFUSE_PUBLIC_KEY, LANGFUSE_SECRET_KEY and LANGFUSE_URL
        let langfuse = send_to_langfuse.then(LangfuseLayer::default);

        tracing_subscriber::registry()
            .with(terminal)
            // Langfuse data is emitted at `debug`, so its layer gets its own, wider filter
            .with(
                langfuse
                    .clone()
                    .map(|layer| layer.with_filter(LevelFilter::DEBUG)),
            )
            .try_init()?;
        // ANCHOR_END: langfuse

        Ok(Self { langfuse })
    }

    /// Sends any buffered Langfuse events.
    pub async fn flush(&self) -> Result<()> {
        if let Some(langfuse) = &self.langfuse {
            langfuse.flush().await?;
        }
        Ok(())
    }
}

/// `RUST_LOG` when set, otherwise [`DEFAULT_LOG_FILTER`].
fn log_filter() -> EnvFilter {
    std::env::var(EnvFilter::DEFAULT_ENV)
        .ok()
        .filter(|directives| !directives.trim().is_empty())
        .map_or_else(|| EnvFilter::new(DEFAULT_LOG_FILTER), EnvFilter::new)
}

/// `LangfuseLayer::default()` panics without keys; fail with a normal error instead. Like that
/// constructor, it accepts either variable of each pair.
fn ensure_langfuse_keys() -> Result<()> {
    const KEYS: [(&str, &str); 2] = [
        ("LANGFUSE_PUBLIC_KEY", "LANGFUSE_INIT_PROJECT_PUBLIC_KEY"),
        ("LANGFUSE_SECRET_KEY", "LANGFUSE_INIT_PROJECT_SECRET_KEY"),
    ];
    let is_set = |key: &str| std::env::var(key).is_ok_and(|value| !value.is_empty());

    for (key, alternative) in KEYS {
        anyhow::ensure!(
            is_set(key) || is_set(alternative),
            "--langfuse needs {key} (or {alternative}), and optionally LANGFUSE_URL, in the \
             environment"
        );
    }
    Ok(())
}
