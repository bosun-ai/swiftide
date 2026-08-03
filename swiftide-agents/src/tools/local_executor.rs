//! Local executor for running tools on the local machine.
//!
//! By default will use the current directory as the working directory.
use std::{
    borrow::Cow,
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use anyhow::Result;
use async_trait::async_trait;
use derive_builder::Builder;
use futures_util::{Stream, StreamExt as _, stream};
#[cfg(windows)]
use process_wrap::tokio::JobObject;
#[cfg(unix)]
use process_wrap::tokio::ProcessGroup;
use process_wrap::tokio::{CommandWrap, KillOnDrop};
use swiftide_core::{
    Command, CommandError, CommandOutput, CommandOutputChunk, Loader, ToolExecutor,
};
use swiftide_indexing::loaders::FileLoader;
use tokio::{io::AsyncWriteExt as _, process::ChildStdin, time};
use tokio_util::io::ReaderStream;

const OUTPUT_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
const DEFAULT_OUTPUT_READ_SIZE: usize = 8 * 1024;

#[derive(Debug, Clone, Builder)]
#[builder(build_fn(validate = "Self::validate"))]
pub struct LocalExecutor {
    #[builder(default = ".".into(), setter(into))]
    workdir: PathBuf,

    #[builder(default)]
    default_timeout: Option<Duration>,

    /// Maximum bytes read from an output pipe per chunk.
    #[builder(default = "DEFAULT_OUTPUT_READ_SIZE")]
    output_read_size: usize,

    /// Clears env variables before executing commands.
    #[builder(default)]
    pub(crate) env_clear: bool,
    /// Remove these environment variables before executing commands.
    #[builder(default, setter(into))]
    pub(crate) env_remove: Vec<String>,
    ///  Set these environment variables before executing commands.
    #[builder(default, setter(into))]
    pub(crate) envs: HashMap<String, String>,
}

impl Default for LocalExecutor {
    fn default() -> Self {
        LocalExecutor {
            workdir: ".".into(),
            default_timeout: None,
            output_read_size: DEFAULT_OUTPUT_READ_SIZE,
            env_clear: false,
            env_remove: Vec::new(),
            envs: HashMap::new(),
        }
    }
}

impl LocalExecutorBuilder {
    fn validate(&self) -> Result<(), String> {
        if self.output_read_size == Some(0) {
            return Err("output read size must be greater than zero".into());
        }

        Ok(())
    }
}

impl LocalExecutor {
    pub fn new(workdir: impl Into<PathBuf>) -> Self {
        LocalExecutor {
            workdir: workdir.into(),
            default_timeout: None,
            output_read_size: DEFAULT_OUTPUT_READ_SIZE,
            env_clear: false,
            env_remove: Vec::new(),
            envs: HashMap::new(),
        }
    }

    pub fn builder() -> LocalExecutorBuilder {
        LocalExecutorBuilder::default()
    }

    fn resolve_workdir<'a>(&'a self, cmd: &'a Command) -> Cow<'a, Path> {
        match cmd.current_dir_path() {
            Some(path) if path.is_absolute() => Cow::Borrowed(path),
            Some(path) => Cow::Owned(self.workdir.join(path)),
            None => Cow::Borrowed(&self.workdir),
        }
    }

    fn resolve_timeout(&self, cmd: &Command) -> Option<Duration> {
        cmd.timeout_duration().copied().or(self.default_timeout)
    }

    async fn exec_shell(
        &self,
        cmd: &str,
        workdir: &Path,
        timeout: Option<Duration>,
    ) -> Result<CommandOutput, CommandError> {
        let (mut command, input) = if let Some(script) = ShellScript::parse(cmd) {
            tracing::info!(interpreter = script.interpreter, "detected shebang");
            let mut command = tokio::process::Command::new(script.interpreter);
            if let Some(argument) = script.argument {
                command.arg(argument);
            }
            (command, Some(script.body.as_bytes()))
        } else {
            tracing::info!("no shebang detected; running as command");
            let mut command = tokio::process::Command::new("sh");
            command.arg("-c").arg(cmd);
            (command, None)
        };

        self.configure_command(&mut command);

        command
            .current_dir(workdir)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut command = CommandWrap::from(command);
        #[cfg(unix)]
        command.wrap(ProcessGroup::leader());
        #[cfg(windows)]
        command.wrap(JobObject);
        command.wrap(KillOnDrop);

        let mut child = command.spawn()?;
        drop(command);
        let stdin = child.stdin().take();
        let stdout = ReaderStream::with_capacity(
            child
                .stdout()
                .take()
                .expect("stdout is configured as piped"),
            self.output_read_size,
        )
        .map(|chunk| chunk.map(CommandOutputChunk::Stdout));
        let stderr = ReaderStream::with_capacity(
            child
                .stderr()
                .take()
                .expect("stderr is configured as piped"),
            self.output_read_size,
        )
        .map(|chunk| chunk.map(CommandOutputChunk::Stderr));
        let mut output_stream = stream::select(stdout, stderr);
        let mut output_chunks = Vec::new();

        let execution = async {
            let write_input = write_input(stdin, input);
            let read_output = collect_output(&mut output_stream, &mut output_chunks);
            let (status, (), ()) = tokio::try_join!(child.wait(), read_output, write_input)?;
            Ok::<_, io::Error>(status)
        };

        let status = match timeout {
            Some(limit) => {
                let Ok(result) = time::timeout(limit, execution).await else {
                    tracing::warn!(?limit, "command exceeded timeout; terminating");
                    if let Err(error) = Box::into_pin(child.kill()).await
                        && error.kind() != io::ErrorKind::InvalidInput
                    {
                        tracing::warn!(?error, "failed to kill command");
                    }
                    drain_output(&mut output_stream, &mut output_chunks).await;

                    return Err(CommandError::TimedOut {
                        timeout: limit,
                        output: CommandOutput::from_chunks(output_chunks),
                    });
                };
                result?
            }
            None => execution.await?,
        };

        let output = CommandOutput::from_chunks(output_chunks);
        if status.success() {
            Ok(output)
        } else {
            Err(CommandError::NonZeroExit(output))
        }
    }

    async fn exec_read_file(
        &self,
        workdir: &Path,
        path: &Path,
        timeout: Option<Duration>,
    ) -> Result<CommandOutput, CommandError> {
        let path = resolve_path(workdir, path);
        let read_future = fs_err::tokio::read(path.as_ref());
        let output = match timeout {
            Some(limit) => match time::timeout(limit, read_future).await {
                Ok(result) => result?,
                Err(_) => {
                    return Err(CommandError::TimedOut {
                        timeout: limit,
                        output: CommandOutput::empty(),
                    });
                }
            },
            None => read_future.await?,
        };

        Ok(output.into())
    }

    async fn exec_write_file(
        &self,
        workdir: &Path,
        path: &Path,
        content: &str,
        timeout: Option<Duration>,
    ) -> Result<CommandOutput, CommandError> {
        let path = resolve_path(workdir, path);
        if let Some(parent) = path.parent() {
            let _ = fs_err::tokio::create_dir_all(parent).await;
        }
        let write_future = fs_err::tokio::write(path.as_ref(), content);
        match timeout {
            Some(limit) => match time::timeout(limit, write_future).await {
                Ok(result) => result?,
                Err(_) => {
                    return Err(CommandError::TimedOut {
                        timeout: limit,
                        output: CommandOutput::empty(),
                    });
                }
            },
            None => write_future.await?,
        }

        Ok(CommandOutput::empty())
    }

    fn configure_command(&self, command: &mut tokio::process::Command) {
        if self.env_clear {
            tracing::info!("clearing environment variables");
            command.env_clear();
        }
        for var in &self.env_remove {
            tracing::info!(var, "clearing environment variable");
            command.env_remove(var);
        }
        for (key, value) in &self.envs {
            tracing::info!(key, "setting environment variable");
            command.env(key, value);
        }
    }
}

struct ShellScript<'a> {
    interpreter: &'a str,
    argument: Option<&'a str>,
    body: &'a str,
}

impl<'a> ShellScript<'a> {
    fn parse(command: &'a str) -> Option<Self> {
        let (shebang, body) = command.split_once('\n').unwrap_or((command, ""));
        let directive = shebang.strip_prefix("#!")?.trim();
        let split = directive.find(char::is_whitespace);
        let (interpreter, argument) = split.map_or((directive, None), |index| {
            let argument = directive[index..].trim();
            (
                &directive[..index],
                (!argument.is_empty()).then_some(argument),
            )
        });

        (!interpreter.is_empty()).then_some(Self {
            interpreter,
            argument,
            body,
        })
    }
}

fn resolve_path<'a>(workdir: &'a Path, path: &'a Path) -> Cow<'a, Path> {
    if path.is_absolute() {
        Cow::Borrowed(path)
    } else {
        Cow::Owned(workdir.join(path))
    }
}

async fn write_input(mut stdin: Option<ChildStdin>, input: Option<&[u8]>) -> io::Result<()> {
    if let (Some(stdin), Some(input)) = (&mut stdin, input) {
        stdin.write_all(input).await?;
    }
    Ok(())
}

async fn collect_output<S>(stream: &mut S, chunks: &mut Vec<CommandOutputChunk>) -> io::Result<()>
where
    S: Stream<Item = io::Result<CommandOutputChunk>> + Unpin,
{
    while let Some(chunk) = stream.next().await {
        chunks.push(chunk?);
    }
    Ok(())
}

async fn drain_output<S>(stream: &mut S, chunks: &mut Vec<CommandOutputChunk>)
where
    S: Stream<Item = io::Result<CommandOutputChunk>> + Unpin,
{
    match time::timeout(OUTPUT_DRAIN_TIMEOUT, collect_output(stream, chunks)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::warn!(?error, "failed to drain command output"),
        Err(_) => tracing::warn!("timed out draining command output"),
    }
}

#[async_trait]
impl ToolExecutor for LocalExecutor {
    /// Execute a `Command` on the local machine
    #[tracing::instrument(skip_self)]
    async fn exec_cmd(&self, cmd: &Command) -> Result<swiftide_core::CommandOutput, CommandError> {
        let workdir = __self.resolve_workdir(cmd);
        let timeout = __self.resolve_timeout(cmd);
        match cmd {
            Command::Shell { command, .. } => __self.exec_shell(command, &workdir, timeout).await,
            Command::ReadFile { path, .. } => __self.exec_read_file(&workdir, path, timeout).await,
            Command::WriteFile { path, content, .. } => {
                __self
                    .exec_write_file(&workdir, path, content, timeout)
                    .await
            }
            _ => unimplemented!("Unsupported command: {cmd:?}"),
        }
    }

    async fn stream_files(
        &self,
        path: &Path,
        extensions: Option<Vec<String>>,
    ) -> Result<swiftide_core::indexing::IndexingStream<String>> {
        let mut loader = FileLoader::new(path);

        if let Some(extensions) = extensions {
            loader = loader.with_extensions(&extensions);
        }

        Ok(loader.into_stream())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;
    use std::{path::Path, sync::Arc, time::Duration};
    use swiftide_core::{Command, ExecutorExt, ToolExecutor};
    use temp_dir::TempDir;

    #[cfg(unix)]
    async fn wait_for_process_exit(pid: i32) -> anyhow::Result<()> {
        time::timeout(Duration::from_secs(2), async {
            while std::process::Command::new("kill")
                .args(["-0", &pid.to_string()])
                .status()
                .is_ok_and(|status| status.success())
            {
                time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_local_executor_write_and_read_file() -> anyhow::Result<()> {
        // Create a temporary directory
        let temp_dir = TempDir::new()?;
        let temp_path = temp_dir.path();

        // Instantiate LocalExecutor with the temporary directory as workdir
        let executor = LocalExecutor {
            workdir: temp_path.to_path_buf(),
            ..Default::default()
        };

        // Define the file path and content
        let file_path = temp_path.join("test_file.txt");
        let file_content = "Hello, world!";

        // Write a shell command to create a file with the specified content
        let write_cmd =
            Command::shell(format!("echo '{}' > {}", file_content, file_path.display()));

        // Execute the write command
        executor.exec_cmd(&write_cmd).await?;

        // Verify that the file was created successfully
        assert!(file_path.exists());

        // Write a shell command to read the file's content
        let read_cmd = Command::shell(format!("cat {}", file_path.display()));

        // Execute the read command
        let output = executor.exec_cmd(&read_cmd).await?;

        // Verify that the content read from the file matches the expected content
        assert_eq!(output.stdout_to_string_lossy(), format!("{file_content}\n"));

        let output = executor
            .exec_cmd(&Command::read_file(&file_path))
            .await
            .unwrap();
        assert_eq!(output.stdout_to_string_lossy(), format!("{file_content}\n"));

        Ok(())
    }

    #[tokio::test]
    async fn test_local_executor_echo_hello_world() -> anyhow::Result<()> {
        // Create a temporary directory
        let temp_dir = TempDir::new()?;
        let temp_path = temp_dir.path();

        // Instantiate LocalExecutor with the temporary directory as workdir
        let executor = LocalExecutor {
            workdir: temp_path.to_path_buf(),
            ..Default::default()
        };

        // Define the echo command
        let echo_cmd = Command::shell("echo 'hello world'");

        // Execute the echo command
        let output = executor.exec_cmd(&echo_cmd).await?;

        // Verify that the output matches the expected content
        assert_eq!(output.stdout_to_string_lossy().trim(), "hello world");

        Ok(())
    }

    #[tokio::test]
    async fn test_local_executor_separates_stdout_and_stderr() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let temp_path = temp_dir.path();

        let executor = LocalExecutor {
            workdir: temp_path.to_path_buf(),
            ..Default::default()
        };

        let output = executor
            .exec_cmd(&Command::shell(
                "printf 'hello stdout'; printf 'hello stderr' >&2",
            ))
            .await?;

        assert_eq!(output.stdout_to_string_lossy(), "hello stdout");
        assert_eq!(output.stderr_to_string_lossy(), "hello stderr");

        Ok(())
    }

    #[tokio::test]
    async fn test_local_executor_uses_configured_output_read_size() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let executor = LocalExecutor::builder()
            .workdir(temp_dir.path().to_path_buf())
            .output_read_size(3)
            .build()?;

        let output = executor
            .exec_cmd(&Command::shell("printf 'abcdef'; printf 'uvwxyz' >&2"))
            .await?;

        assert!(
            output
                .chunks()
                .iter()
                .all(|chunk| chunk.as_bytes().len() <= 3)
        );
        assert_eq!(output.stdout_to_string_lossy(), "abcdef");
        assert_eq!(output.stderr_to_string_lossy(), "uvwxyz");

        Ok(())
    }

    #[test]
    fn test_local_executor_rejects_zero_output_read_size() {
        assert!(
            LocalExecutor::builder()
                .output_read_size(0)
                .build()
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_local_executor_preserves_stderr_only_failure() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let temp_path = temp_dir.path();

        let executor = LocalExecutor {
            workdir: temp_path.to_path_buf(),
            ..Default::default()
        };

        match executor
            .exec_cmd(&Command::shell("printf 'boom' >&2; exit 1"))
            .await
        {
            Err(CommandError::NonZeroExit(output)) => {
                assert!(output.stdout_to_string_lossy().is_empty());
                assert_eq!(output.stderr_to_string_lossy(), "boom");
            }
            other => anyhow::bail!("expected non-zero exit, got {other:?}"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_local_executor_preserves_order_within_each_stream() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let executor = LocalExecutor {
            workdir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };

        let output = executor
            .exec_cmd(&Command::shell(
                "printf 'one'; printf 'two' >&2; printf 'three'",
            ))
            .await?;

        assert_eq!(output.stdout_to_string_lossy(), "onethree");
        assert_eq!(output.stderr_to_string_lossy(), "two");

        Ok(())
    }

    #[tokio::test]
    async fn test_local_executor_retains_observed_chunk_order() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let executor = LocalExecutor::new(temp_dir.path());

        let output = executor
            .exec_cmd(&Command::shell(
                "printf 'one'; sleep 0.05; printf 'two' >&2; sleep 0.05; printf 'three'",
            ))
            .await?;

        assert_eq!(output.to_string_lossy(), "onetwothree");
        assert!(matches!(
            output.chunks(),
            [
                CommandOutputChunk::Stdout(_),
                CommandOutputChunk::Stderr(_),
                CommandOutputChunk::Stdout(_)
            ]
        ));

        Ok(())
    }

    #[tokio::test]
    async fn test_local_executor_does_not_invent_or_remove_newlines() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let executor = LocalExecutor::new(temp_dir.path());

        let output = executor
            .exec_cmd(&Command::shell(
                "printf 'one\\n'; printf 'two\\r\\n' >&2; printf '\\n'",
            ))
            .await?;

        assert_eq!(output.stdout_to_string_lossy(), "one\n\n");
        assert_eq!(output.stderr_to_string_lossy(), "two\r\n");

        Ok(())
    }

    #[tokio::test]
    async fn test_local_executor_preserves_partial_output_on_timeout() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let executor = LocalExecutor {
            workdir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let command = Command::shell("printf 'one'; printf 'two' >&2; sleep 1")
            .with_timeout(Duration::from_millis(100));

        match executor.exec_cmd(&command).await {
            Err(CommandError::TimedOut { output, .. }) => {
                assert_eq!(output.stdout_to_string_lossy(), "one");
                assert_eq!(output.stderr_to_string_lossy(), "two");
            }
            other => anyhow::bail!("expected timeout, got {other:?}"),
        }

        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn timing_out_execution_kills_spawned_processes() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let pid_file = temp_dir.path().join("child.pid");
        let executor = LocalExecutor::new(temp_dir.path());
        let command = Command::shell(format!(
            "sleep 30 & echo $! > '{}'; wait",
            pid_file.display()
        ))
        .with_timeout(Duration::from_millis(500));

        assert!(matches!(
            executor.exec_cmd(&command).await,
            Err(CommandError::TimedOut { .. })
        ));
        let pid = fs_err::tokio::read_to_string(pid_file)
            .await?
            .trim()
            .parse()?;
        wait_for_process_exit(pid).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_local_executor_passes_shebang_argument() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let executor = LocalExecutor::new(temp_dir.path());

        match executor
            .exec_cmd(&Command::shell(
                "#!/bin/sh -e\nprintf before\nfalse\nprintf after",
            ))
            .await
        {
            Err(CommandError::NonZeroExit(output)) => {
                assert_eq!(output.stdout_to_string_lossy(), "before");
                assert!(output.stderr_to_string_lossy().is_empty());
            }
            other => anyhow::bail!("expected non-zero exit, got {other:?}"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_local_executor_does_not_wait_for_redirected_background_processes()
    -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let executor = LocalExecutor::new(temp_dir.path());
        let command =
            Command::shell("sleep 5 >/dev/null 2>&1 &").with_timeout(Duration::from_millis(500));

        let output = executor.exec_cmd(&command).await?;

        assert!(output.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_local_executor_shell_timeout() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let temp_path = temp_dir.path();

        let executor = LocalExecutor {
            workdir: temp_path.to_path_buf(),
            ..Default::default()
        };

        let mut cmd = Command::shell("echo ready && sleep 1 && echo done");
        cmd.timeout(Duration::from_millis(100));

        match executor.exec_cmd(&cmd).await {
            Err(CommandError::TimedOut { timeout, output }) => {
                assert_eq!(timeout, Duration::from_millis(100));
                assert!(output.stdout_to_string_lossy().contains("ready"));
            }
            other => anyhow::bail!("expected timeout error, got {other:?}"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_local_executor_default_timeout_applies() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let temp_path = temp_dir.path();

        let executor = LocalExecutorBuilder::default()
            .workdir(temp_path.to_path_buf())
            .default_timeout(Some(Duration::from_millis(100)))
            .build()?;

        match executor.exec_cmd(&Command::shell("sleep 1")).await {
            Err(CommandError::TimedOut { timeout, output }) => {
                assert_eq!(timeout, Duration::from_millis(100));
                assert!(output.is_empty());
            }
            other => anyhow::bail!("expected default timeout, got {other:?}"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_local_executor_clear_env() -> anyhow::Result<()> {
        // Create a temporary directory
        let temp_dir = TempDir::new()?;
        let temp_path = temp_dir.path();

        // Instantiate LocalExecutor with the temporary directory as workdir
        let executor = LocalExecutor {
            workdir: temp_path.to_path_buf(),
            env_clear: true,
            ..Default::default()
        };

        // Define the echo command
        let echo_cmd = Command::shell("printenv");

        // Execute the echo command
        let result = executor.exec_cmd(&echo_cmd).await?;
        let output = result.stdout_to_string_lossy();

        // Verify that the output matches the expected content
        // assert_eq!(output.to_string().trim(), "");
        assert!(!output.contains("CARGO_PKG_VERSION"), "{output}");

        Ok(())
    }

    #[tokio::test]
    async fn test_local_executor_add_env() -> anyhow::Result<()> {
        // Create a temporary directory
        let temp_dir = TempDir::new()?;
        let temp_path = temp_dir.path();

        // Instantiate LocalExecutor with the temporary directory as workdir
        let executor = LocalExecutor {
            workdir: temp_path.to_path_buf(),
            envs: HashMap::from([("TEST_ENV".to_string(), "HELLO".to_string())]),
            ..Default::default()
        };

        // Define the echo command
        let echo_cmd = Command::shell("printenv");

        // Execute the echo command
        let result = executor.exec_cmd(&echo_cmd).await?;
        let output = result.stdout_to_string_lossy();

        // Verify that the output matches the expected content
        // assert_eq!(output.to_string().trim(), "");
        assert!(output.contains("TEST_ENV=HELLO"), "{output}");
        // Double tap its included by default
        assert!(output.contains("CARGO_PKG_VERSION"), "{output}");

        Ok(())
    }

    #[tokio::test]
    async fn test_local_executor_env_remove() -> anyhow::Result<()> {
        // Create a temporary directory
        let temp_dir = TempDir::new()?;
        let temp_path = temp_dir.path();

        // Instantiate LocalExecutor with the temporary directory as workdir
        let executor = LocalExecutor {
            workdir: temp_path.to_path_buf(),
            env_remove: vec!["CARGO_PKG_VERSION".to_string()],
            ..Default::default()
        };

        // Define the echo command
        let echo_cmd = Command::shell("printenv");

        // Execute the echo command
        let result = executor.exec_cmd(&echo_cmd).await?;
        let output = result.stdout_to_string_lossy();

        // Verify that the output matches the expected content
        // assert_eq!(output.to_string().trim(), "");
        assert!(!output.contains("CARGO_PKG_VERSION="), "{output}");

        Ok(())
    }

    #[tokio::test]
    async fn test_local_executor_run_shebang() -> anyhow::Result<()> {
        // Create a temporary directory
        let temp_dir = TempDir::new()?;
        let temp_path = temp_dir.path();

        // Instantiate LocalExecutor with the temporary directory as workdir
        let executor = LocalExecutor {
            workdir: temp_path.to_path_buf(),
            ..Default::default()
        };

        let script = r#"#!/usr/bin/env python3
print("hello from python")
print(1 + 2)"#;

        // Execute the echo command
        let result = executor.exec_cmd(&Command::shell(script)).await?;
        let output = result.stdout_to_string_lossy();

        // Verify that the output matches the expected content
        assert!(output.contains("hello from python"));
        assert!(output.contains('3'));

        Ok(())
    }

    #[tokio::test]
    async fn test_local_executor_multiline_with_quotes() -> anyhow::Result<()> {
        // Create a temporary directory
        let temp_dir = TempDir::new()?;
        let temp_path = temp_dir.path();

        // Instantiate LocalExecutor with the temporary directory as workdir
        let executor = LocalExecutor {
            workdir: temp_path.to_path_buf(),
            ..Default::default()
        };

        // Define the file path and content
        let file_path = "test_file2.txt";
        let file_content = indoc! {r#"
            fn main() {
                println!("Hello, world!");
            }
        "#};

        // Write a shell command to create a file with the specified content
        let write_cmd = Command::shell(format!("echo '{file_content}' > {file_path}"));

        // Execute the write command
        executor.exec_cmd(&write_cmd).await?;

        // Write a shell command to read the file's content
        let read_cmd = Command::shell(format!("cat {file_path}"));

        // Execute the read command
        let output = executor.exec_cmd(&read_cmd).await?;

        // Verify that the content read from the file matches the expected content
        assert_eq!(output.stdout_to_string_lossy(), format!("{file_content}\n"));

        Ok(())
    }

    #[tokio::test]
    async fn test_local_executor_write_and_read_file_commands() -> anyhow::Result<()> {
        // Create a temporary directory
        let temp_dir = TempDir::new()?;
        let temp_path = temp_dir.path();

        // Instantiate LocalExecutor with the temporary directory as workdir
        let executor = LocalExecutor {
            workdir: temp_path.to_path_buf(),
            ..Default::default()
        };

        // Define the file path and content
        let file_path = temp_path.join("test_file.txt");
        let file_content = "Hello, world!";

        // Assert that the file does not exist and it gives the correct error
        let cmd = Command::read_file(file_path.clone());
        let result = executor.exec_cmd(&cmd).await;

        if let Err(err) = result {
            assert!(matches!(err, CommandError::ExecutorError(..)));
        } else {
            panic!("Expected error but got {result:?}");
        }

        // Create a write command
        let write_cmd = Command::write_file(file_path.clone(), file_content.to_string());

        // Execute the write command
        executor.exec_cmd(&write_cmd).await?;

        // Verify that the file was created successfully
        assert!(file_path.exists());

        // Create a read command
        let read_cmd = Command::read_file(file_path.clone());

        // Execute the read command
        let output = executor
            .exec_cmd(&read_cmd)
            .await?
            .stdout_to_string_lossy()
            .into_owned();

        // Verify that the content read from the file matches the expected content
        assert_eq!(output, file_content);

        Ok(())
    }

    #[tokio::test]
    async fn read_file_preserves_invalid_utf8() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let file = temp_dir.path().join("bytes.bin");
        fs_err::tokio::write(&file, b"valid\xff").await?;
        let executor = LocalExecutor::new(temp_dir.path());

        let output = executor.exec_cmd(&Command::read_file(file)).await?;

        assert_eq!(output.chunks().len(), 1);
        assert_eq!(output.chunks()[0].as_bytes(), b"valid\xff");
        assert!(output.stderr_to_string_lossy().is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_local_executor_stream_files() -> anyhow::Result<()> {
        // Create a temporary directory
        let temp_dir = TempDir::new()?;
        let temp_path = temp_dir.path();

        // Create some test files in the temporary directory
        fs_err::write(temp_path.join("file1.txt"), "Content of file 1")?;
        fs_err::write(temp_path.join("file2.txt"), "Content of file 2")?;
        fs_err::write(temp_path.join("file3.rs"), "Content of file 3")?;

        // Instantiate LocalExecutor with the temporary directory as workdir
        let executor = LocalExecutor {
            workdir: temp_path.to_path_buf(),
            ..Default::default()
        };

        // Stream files with no extensions filter
        let stream = executor.stream_files(temp_path, None).await?;
        let files: Vec<_> = stream.collect().await;

        assert_eq!(files.len(), 3);

        // Stream files with a specific extension filter
        let stream = executor
            .stream_files(temp_path, Some(vec!["txt".to_string()]))
            .await?;
        let txt_files: Vec<_> = stream.collect().await;

        assert_eq!(txt_files.len(), 2);

        Ok(())
    }

    #[tokio::test]
    async fn test_local_executor_honors_workdir() -> anyhow::Result<()> {
        use std::fs;
        use temp_dir::TempDir;

        // 1. Create a temp dir and instantiate executor
        let temp_dir = TempDir::new()?;
        let temp_path = temp_dir.path();

        let executor = LocalExecutor {
            workdir: temp_path.to_path_buf(),
            ..Default::default()
        };

        // 2. Run a shell command in workdir and check output is workdir
        let pwd_cmd = Command::shell("pwd");
        let pwd_result = executor.exec_cmd(&pwd_cmd).await?;
        let pwd_output = pwd_result.stdout_to_string_lossy();
        let pwd_path = std::fs::canonicalize(pwd_output.trim())?;
        let temp_path = std::fs::canonicalize(temp_path)?;
        assert_eq!(pwd_path, temp_path);

        // 3. Write a file using WriteFile (should land in workdir)
        let fname = "workdir_check.txt";
        let write_cmd = Command::write_file(fname, "test123");
        executor.exec_cmd(&write_cmd).await?;

        // 4. Assert file exists in workdir, not current dir
        let expected_path = temp_path.join(fname);
        assert!(expected_path.exists());
        assert!(!Path::new(fname).exists());

        // 5. Write/read using ReadFile
        let read_cmd = Command::read_file(fname);
        let read_result = executor.exec_cmd(&read_cmd).await?;
        let read_output = read_result.stdout_to_string_lossy();
        assert_eq!(read_output.trim(), "test123");

        // 6. Clean up
        fs::remove_file(&expected_path)?;

        Ok(())
    }

    #[tokio::test]
    async fn test_local_executor_command_current_dir() -> anyhow::Result<()> {
        use std::fs;
        use temp_dir::TempDir;

        let temp_dir = TempDir::new()?;
        let base_path = temp_dir.path();

        let executor = LocalExecutor {
            workdir: base_path.to_path_buf(),
            ..Default::default()
        };

        let nested_dir = base_path.join("nested");
        fs::create_dir_all(&nested_dir)?;

        let mut pwd_cmd = Command::shell("pwd");
        pwd_cmd.current_dir(Path::new("nested"));
        let pwd_result = executor.exec_cmd(&pwd_cmd).await?;
        let pwd_output = pwd_result.stdout_to_string_lossy();
        let pwd_path = std::fs::canonicalize(pwd_output.trim())?;
        assert_eq!(pwd_path, std::fs::canonicalize(&nested_dir)?);

        let mut write_cmd = Command::write_file("file.txt", "hello");
        write_cmd.current_dir(Path::new("nested"));
        executor.exec_cmd(&write_cmd).await?;

        assert!(!base_path.join("file.txt").exists());
        assert!(nested_dir.join("file.txt").exists());

        let mut read_cmd = Command::read_file("file.txt");
        read_cmd.current_dir(Path::new("nested"));
        let read_result = executor.exec_cmd(&read_cmd).await?;
        let read_output = read_result.stdout_to_string_lossy();
        assert_eq!(read_output.trim(), "hello");

        Ok(())
    }

    #[tokio::test]
    async fn test_local_executor_current_dir() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let base_path = temp_dir.path();

        let executor = LocalExecutor {
            workdir: base_path.to_path_buf(),
            ..Default::default()
        };

        let nested = executor.scoped("nested");
        nested
            .exec_cmd(&Command::write_file("file.txt", "hello"))
            .await?;

        assert!(!base_path.join("file.txt").exists());
        assert!(base_path.join("nested").join("file.txt").exists());
        assert_eq!(executor.workdir, base_path);

        Ok(())
    }

    #[tokio::test]
    async fn test_local_executor_current_dir_dyn() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let base_path = temp_dir.path();

        let executor = LocalExecutor {
            workdir: base_path.to_path_buf(),
            ..Default::default()
        };

        let dyn_exec: Arc<dyn swiftide_core::ToolExecutor> = Arc::new(executor.clone());
        let nested = dyn_exec.scoped("nested");

        nested
            .exec_cmd(&Command::write_file("nested_file.txt", "hello"))
            .await?;

        assert!(base_path.join("nested").join("nested_file.txt").exists());
        assert!(!base_path.join("nested_file.txt").exists());

        Ok(())
    }
}
