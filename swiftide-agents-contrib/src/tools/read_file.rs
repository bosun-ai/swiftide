use std::{
    borrow::Cow,
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Weak},
};

use async_trait::async_trait;
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use shell_escape::escape;
use swiftide_core::{
    AgentContext, Command,
    chat_completion::{Tool, ToolCall, ToolOutput, ToolSpecBuilder, errors::ToolError},
};
use tokio::sync::Mutex as AsyncMutex;

const DEFAULT_READ_LIMIT: usize = 2_000;
const MAX_READ_LIMIT: usize = 100_000;
const MAX_READ_CHARACTERS: usize = 100_000;
const READ_META_PREFIX: &str = "__QUAK_CLAUDE_READ_META__";

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileFingerprint {
    size: u64,
    modified: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum FileKind {
    File,
    Symlink,
    Missing,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileInfo {
    kind: FileKind,
    fingerprint: Option<FileFingerprint>,
}

#[derive(Clone, Debug)]
struct ReadState {
    fingerprint: FileFingerprint,
    complete: bool,
    covered_ranges: Vec<(usize, usize)>,
    eof_line: Option<usize>,
}

impl ReadState {
    fn new(fingerprint: FileFingerprint, complete: bool) -> Self {
        Self {
            fingerprint,
            complete,
            covered_ranges: Vec::new(),
            eof_line: None,
        }
    }

    fn record(&mut self, observation: ReadObservation) {
        if let Some(range) = observation.range {
            self.covered_ranges.push(range);
            self.covered_ranges.sort_unstable_by_key(|range| range.0);
            self.covered_ranges = self.covered_ranges.drain(..).fold(
                Vec::<(usize, usize)>::new(),
                |mut merged, range| {
                    if let Some(previous) = merged.last_mut()
                        && range.0 <= previous.1.saturating_add(1)
                    {
                        previous.1 = previous.1.max(range.1);
                    } else {
                        merged.push(range);
                    }
                    merged
                },
            );
        }
        if observation.eof_line.is_some() {
            self.eof_line = observation.eof_line;
        }
        self.complete |= match self.eof_line {
            Some(0) => true,
            Some(eof_line) => self
                .covered_ranges
                .first()
                .is_some_and(|&(start, end)| start == 1 && end >= eof_line),
            None => false,
        };
    }
}

#[derive(Debug, Default)]
pub(crate) struct FileReadState {
    reads: Mutex<HashMap<(usize, PathBuf), ReadState>>,
    file_locks: Mutex<HashMap<PathBuf, Weak<AsyncMutex<()>>>>,
}

impl FileReadState {
    fn file_lock(&self, path: &Path) -> Arc<AsyncMutex<()>> {
        let mut locks = self.file_locks.lock().expect("file lock map poisoned");
        if let Some(lock) = locks.get(path).and_then(Weak::upgrade) {
            return lock;
        }

        let lock = Arc::new(AsyncMutex::new(()));
        locks.insert(path.to_path_buf(), Arc::downgrade(&lock));
        lock
    }

    fn record_page(
        &self,
        context: &dyn AgentContext,
        path: &Path,
        fingerprint: FileFingerprint,
        observation: ReadObservation,
    ) {
        let mut reads = self.reads.lock().expect("read state map poisoned");
        let state = reads
            .entry((context_id(context), path.to_path_buf()))
            .or_insert_with(|| ReadState::new(fingerprint.clone(), false));
        if state.fingerprint != fingerprint {
            *state = ReadState::new(fingerprint, false);
        }
        state.record(observation);
    }
}

fn context_id(context: &dyn AgentContext) -> usize {
    std::ptr::from_ref(context).cast::<()>() as usize
}

/// Fluyt's capped `Read` tool, preserving its arguments, formatting, and read-state checks.
///
/// Uses absolute `file_path`, optional one-based `offset` (zero also starts at one), and optional
/// `limit`. Defaults to 2,000 lines, with the original 100,000-character budget calculation.
/// The tool checks file metadata before and after reads and retains page coverage in its state.
/// Empty files, past-end reads, partial pages, and errors use the source tool's messages.
///
/// # Examples
///
/// ```no_run
/// use swiftide_agents::Agent;
/// use swiftide_agents_contrib::tools::ReadFile;
/// Agent::builder().add_tool(ReadFile::default());
/// ```
#[derive(Clone, Debug)]
pub struct ReadFile {
    state: Arc<FileReadState>,
}

impl Default for ReadFile {
    fn default() -> Self {
        Self::with_state(Arc::default())
    }
}

impl ReadFile {
    pub(crate) fn with_state(state: Arc<FileReadState>) -> Self {
        Self { state }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReadArgs {
    /// The absolute path to the file to read
    file_path: String,
    #[schemars(
        description = "The line number to start reading from. Only provide if the file is too large to read at once"
    )]
    offset: Option<usize>,
    /// The number of lines to read. Only provide if the file is too large to read at once.
    limit: Option<usize>,
}

#[async_trait]
impl Tool for ReadFile {
    async fn invoke(
        &self,
        agent_context: &dyn AgentContext,
        tool_call: &ToolCall,
    ) -> Result<ToolOutput, ToolError> {
        let args = tool_call
            .args()
            .ok_or_else(|| ToolError::missing_arguments("Read"))?;
        let payload: ReadArgs = serde_json::from_str(args)?;
        if let Some(error) = absolute_path_error(&payload.file_path) {
            return Ok(ToolOutput::fail(error));
        }

        let path = Path::new(&payload.file_path);
        let lock = self.state.file_lock(path);
        let _guard = lock.lock().await;
        let before = file_info(agent_context, path).await?;
        let Some(before_fingerprint) = before.fingerprint.as_ref() else {
            return Ok(ToolOutput::fail(format!(
                "{} is not a regular file",
                path.display()
            )));
        };
        let page = match read_text_page(
            agent_context,
            path,
            payload.offset,
            payload.limit,
            before_fingerprint.size,
        )
        .await?
        {
            ReadPage::Content {
                rendered,
                observation,
            } => {
                let after = file_info(agent_context, path).await?;
                if before != after {
                    return Ok(ToolOutput::fail(format!(
                        "{} changed while it was being read; retry the Read",
                        path.display()
                    )));
                }
                let Some(fingerprint) = after.fingerprint else {
                    return Ok(ToolOutput::fail(format!(
                        "{} is not a regular file",
                        path.display()
                    )));
                };
                self.state
                    .record_page(agent_context, path, fingerprint, observation);
                rendered
            }
            ReadPage::Failure(message) => return Ok(ToolOutput::fail(message)),
            ReadPage::PastEnd => unreachable!("past-end pages are rendered before returning"),
        };

        Ok(ToolOutput::text(page))
    }

    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("Read")
    }

    fn tool_spec(&self) -> swiftide_core::chat_completion::ToolSpec {
        ToolSpecBuilder::default()
            .name("Read")
            .description(
                "Reads a file from the local filesystem. Use absolute file paths. The output includes line numbers.",
            )
            .parameters_schema(schema_for!(ReadArgs))
            .build()
            .expect("tool spec is valid")
    }
}

fn absolute_path_error(file_path: &str) -> Option<String> {
    (!Path::new(file_path).is_absolute())
        .then(|| format!("file_path must be an absolute path, but received `{file_path}`"))
}

#[derive(Debug)]
enum ReadPage {
    Content {
        rendered: String,
        observation: ReadObservation,
    },
    Failure(String),
    PastEnd,
}

#[derive(Clone, Copy, Debug, Default)]
struct ReadObservation {
    range: Option<(usize, usize)>,
    eof_line: Option<usize>,
}

impl ReadObservation {
    fn page(start: usize, last: usize, emitted: usize, reached_eof: bool) -> Self {
        Self {
            range: (emitted > 0).then_some((start, last)),
            eof_line: reached_eof.then_some(last),
        }
    }
}

async fn read_text_page(
    context: &dyn AgentContext,
    path: &Path,
    offset: Option<usize>,
    limit: Option<usize>,
    file_size: u64,
) -> Result<ReadPage, ToolError> {
    let start = offset.filter(|offset| *offset > 0).unwrap_or(1);

    if limit.is_some_and(|limit| limit > MAX_READ_LIMIT) {
        return Ok(ReadPage::Failure(format!(
            "limit must not exceed {MAX_READ_LIMIT}; use multiple Read calls"
        )));
    }
    let line_limit = limit.unwrap_or(DEFAULT_READ_LIMIT);
    if line_limit == 0 {
        return Ok(ReadPage::Failure("limit must be 1 or greater".to_string()));
    }

    let explicit_range = offset.is_some() || limit.is_some();
    let escaped_path = escape(Cow::Owned(path.display().to_string()));
    let byte_limit = MAX_READ_CHARACTERS + 1;
    let explicit = usize::from(explicit_range);
    let command = format!(
        "LC_ALL=C tail -n +{start} {escaped_path} | head -c {byte_limit} | \
         awk -v start={start} -v max_lines={line_limit} -v max_chars={MAX_READ_CHARACTERS} -v explicit={explicit} "
    ) + r#"'
BEGIN { emitted=0; chars=0; partial=0; overflow=0; last=0 }
emitted >= max_lines { partial=1; exit }
{
    rendered_length=length($0)+16
    if (rendered_length > max_chars) {
        overflow=1
        exit
    }
    if (chars+rendered_length > max_chars) {
        if (explicit) overflow=1
        else partial=1
        exit
    }
    line_number=start+NR-1
    printf "%6d→%s\n", line_number, $0
    emitted++
    chars+=rendered_length
    last=line_number
}
END {
    if (NR==0) status="empty-range"
    else if (overflow) status="overflow"
    else if (partial) status="partial"
    else status="complete"
    printf "__QUAK_CLAUDE_READ_META__\t%s\t%d\t%d\n", status, last, emitted
}
' "#;
    let output = context
        .executor()
        .exec_cmd(&Command::shell(command))
        .await?
        .into_string_lossy();
    let page = parse_read_page(&output, path, start, explicit_range, file_size)?;
    if !matches!(page, ReadPage::PastEnd) {
        return Ok(page);
    }

    let total_lines = count_file_lines(context, path).await?;
    Ok(ReadPage::Content {
        rendered: format!(
            "The file has {total_lines} lines; offset {start} is past the end of the file."
        ),
        observation: ReadObservation::default(),
    })
}

fn parse_read_page(
    output: &str,
    path: &Path,
    start: usize,
    explicit_range: bool,
    file_size: u64,
) -> Result<ReadPage, ToolError> {
    let marker = output.rfind(READ_META_PREFIX).ok_or_else(|| {
        ToolError::unknown(anyhow::anyhow!(
            "Read did not return page metadata for {}",
            path.display()
        ))
    })?;
    let rendered = output[..marker].trim_end_matches('\n');
    let metadata = output[marker..].split_whitespace().collect::<Vec<_>>();
    if metadata.len() != 4 {
        return Err(ToolError::unknown(anyhow::anyhow!(
            "Read returned invalid page metadata for {}",
            path.display()
        )));
    }
    let status = metadata[1];
    let last = metadata[2].parse::<usize>().map_err(anyhow::Error::from)?;
    let emitted = metadata[3].parse::<usize>().map_err(anyhow::Error::from)?;

    Ok(match status {
        "empty-range" if file_size == 0 => ReadPage::Content {
            rendered: "File exists but has empty contents.".to_string(),
            observation: ReadObservation {
                range: None,
                eof_line: (start == 1).then_some(0),
            },
        },
        "empty-range" => ReadPage::PastEnd,
        "overflow" => ReadPage::Failure(if explicit_range {
            format!(
                "The requested range in {} exceeds the Read output limit. Use a smaller limit or search for specific content.",
                path.display()
            )
        } else {
            format!(
                "A line in {} exceeds the Read output limit. Search for specific content instead.",
                path.display()
            )
        }),
        "partial" => {
            if explicit_range {
                return Ok(ReadPage::Content {
                    rendered: rendered.to_string(),
                    observation: ReadObservation::page(start, last, emitted, false),
                });
            }
            let next = last.saturating_add(1).max(start);
            let content = if rendered.is_empty() {
                format!(
                    "[PARTIAL view: no complete line fit in this page. Continue with offset={next} and a smaller limit, or search for specific content.]"
                )
            } else {
                format!(
                    "{rendered}\n\n[PARTIAL view: showing lines {start}-{last}. Continue with offset={next} and a smaller limit.]"
                )
            };
            ReadPage::Content {
                rendered: content,
                observation: ReadObservation::page(start, last, emitted, false),
            }
        }
        "complete" => ReadPage::Content {
            rendered: rendered.to_string(),
            observation: ReadObservation::page(start, last, emitted, true),
        },
        _ => {
            return Err(ToolError::unknown(anyhow::anyhow!(
                "Read returned unknown page status `{status}` for {}",
                path.display()
            )));
        }
    })
}

async fn count_file_lines(context: &dyn AgentContext, path: &Path) -> Result<usize, ToolError> {
    let escaped_path = escape(Cow::Owned(path.display().to_string()));
    let command = format!(
        "line_count=$(wc -l < {escaped_path}) || exit; \
         if [ -s {escaped_path} ] && [ \"$(tail -c 1 {escaped_path} | wc -l)\" -eq 0 ]; \
         then line_count=$((line_count + 1)); fi; printf '%s\\n' \"$line_count\""
    );
    let output = context
        .executor()
        .exec_cmd(&Command::shell(command))
        .await?
        .into_string_lossy();
    output
        .trim()
        .parse::<usize>()
        .map_err(anyhow::Error::from)
        .map_err(ToolError::from)
}

async fn file_info(context: &dyn AgentContext, path: &Path) -> Result<FileInfo, ToolError> {
    let escaped_path = escape(Cow::Owned(path.display().to_string()));
    let command = format!(
        "if [ ! -e {escaped_path} ] && [ ! -L {escaped_path} ]; then printf 'missing\\n'; \
         elif [ ! -f {escaped_path} ]; then printf 'other\\n'; \
         else if [ -L {escaped_path} ]; then printf 'symlink|'; else printf 'file|'; fi; \
         if stat -c '%s|%y|%Z' {escaped_path} >/dev/null 2>&1; \
         then stat -c '%s|%y|%Z' {escaped_path}; \
         else stat -f '%z|%m|%c' {escaped_path}; fi; fi"
    );
    let output = context
        .executor()
        .exec_cmd(&Command::shell(command))
        .await?
        .into_string_lossy();
    let trimmed = output.trim();
    let fields = trimmed.splitn(4, '|').collect::<Vec<_>>();
    let kind = match fields.first().copied() {
        Some("file") => FileKind::File,
        Some("symlink") => FileKind::Symlink,
        Some("missing") => FileKind::Missing,
        Some("other") => FileKind::Other,
        _ => {
            return Err(ToolError::unknown(anyhow::anyhow!(
                "Could not inspect {}",
                path.display()
            )));
        }
    };
    let fingerprint = if matches!(kind, FileKind::File | FileKind::Symlink) {
        if fields.len() != 4 {
            return Err(ToolError::unknown(anyhow::anyhow!(
                "Could not fingerprint {}",
                path.display()
            )));
        }
        Some(FileFingerprint {
            size: fields[1].parse::<u64>().map_err(anyhow::Error::from)?,
            modified: format!("{}|{}", fields[2], fields[3]),
        })
    } else {
        None
    };

    Ok(FileInfo { kind, fingerprint })
}
