//! Local executor for running tools on the local machine.
//!
//! By default will use the current directory as the working directory.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use derive_builder::Builder;
use swiftide_core::{Command, CommandError, CommandOutput, Loader, ToolExecutor};
use swiftide_indexing::loaders::FileLoader;
use tokio::{
    io::{AsyncBufReadExt as _, AsyncWriteExt as _},
    task::JoinHandle,
    time,
};

#[derive(Debug, Clone, Builder)]
pub struct LocalExecutor {
    #[builder(default = ".".into(), setter(into))]
    workdir: PathBuf,

    #[builder(default)]
    default_timeout: Option<Duration>,

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
            env_clear: false,
            env_remove: Vec::new(),
            envs: HashMap::new(),
        }
    }
}

impl LocalExecutor {
    pub fn new(workdir: impl Into<PathBuf>) -> Self {
        LocalExecutor {
            workdir: workdir.into(),
            default_timeout: None,
            env_clear: false,
            env_remove: Vec::new(),
            envs: HashMap::new(),
        }
    }

    pub fn builder() -> LocalExecutorBuilder {
        LocalExecutorBuilder::default()
    }

    fn resolve_workdir(&self, cmd: &Command) -> PathBuf {
        match cmd.current_dir_path() {
            Some(path) if path.is_absolute() => path.to_path_buf(),
            Some(path) => self.workdir.join(path),
            None => self.workdir.clone(),
        }
    }

    fn resolve_timeout(&self, cmd: &Command) -> Option<Duration> {
        cmd.timeout_duration().copied().or(self.default_timeout)
    }

    #[allow(clippy::too_many_lines)]
    async fn exec_shell(
        &self,
        cmd: &str,
        workdir: &Path,
        timeout: Option<Duration>,
    ) -> Result<CommandOutput, CommandError> {
        let lines: Vec<&str> = cmd.lines().collect();
        let mut child = if let Some(first_line) = lines.first()
            && first_line.starts_with("#!")
        {
            let interpreter = first_line.trim_start_matches("#!/usr/bin/env ").trim();
            tracing::info!(interpreter, "detected shebang; running as script");

            let mut command = tokio::process::Command::new(interpreter);

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

            let mut child = command
                .current_dir(workdir)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;

            if let Some(mut stdin) = child.stdin.take() {
                let body = lines[1..].join("\n");
                stdin.write_all(body.as_bytes()).await?;
            }

            child
        } else {
            tracing::info!("no shebang detected; running as command");

            let mut command = tokio::process::Command::new("sh");

            // Treat as shell command
            command.arg("-c").arg(cmd).current_dir(workdir);

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
            command
                .current_dir(workdir)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?
        };

        let stdout_task = if let Some(stdout) = child.stdout.take() {
            Some(tokio::spawn(async move {
                let mut lines = tokio::io::BufReader::new(stdout).lines();
                let mut out = Vec::new();
                while let Ok(Some(line)) = lines.next_line().await {
                    out.push(line);
                }
                out
            }))
        } else {
            tracing::warn!("Command has no stdout");
            None
        };

        let stderr_task = if let Some(stderr) = child.stderr.take() {
            Some(tokio::spawn(async move {
                let mut lines = tokio::io::BufReader::new(stderr).lines();
                let mut out = Vec::new();
                while let Ok(Some(line)) = lines.next_line().await {
                    out.push(line);
                }
                out
            }))
        } else {
            tracing::warn!("Command has no stderr");
            None
        };

        let status = match timeout {
            Some(limit) => {
                if let Ok(result) = time::timeout(limit, child.wait()).await {
                    result.map_err(|err| CommandError::ExecutorError(err.into()))?
                } else {
                    tracing::warn!(?limit, "command exceeded timeout; terminating");
                    if let Err(err) = child.start_kill() {
                        tracing::warn!(?err, "failed to start kill on timed out command");
                    }
                    if let Err(err) = child.wait().await {
                        tracing::warn!(?err, "failed to reap command after timeout");
                    }

                    let (stdout, stderr) =
                        Self::collect_process_output(stdout_task, stderr_task).await;
                    let cmd_output = Self::merge_output(&stdout, &stderr);

                    return Err(CommandError::TimedOut {
                        timeout: limit,
                        output: cmd_output,
                    });
                }
            }
            None => child
                .wait()
                .await
                .map_err(|err| CommandError::ExecutorError(err.into()))?,
        };

        let (stdout, stderr) = Self::collect_process_output(stdout_task, stderr_task).await;
        let cmd_output = Self::merge_output(&stdout, &stderr);

        if status.success() {
            Ok(cmd_output)
        } else {
            Err(CommandError::NonZeroExit(cmd_output))
        }
    }

    async fn exec_read_file(
        &self,
        workdir: &Path,
        path: &Path,
        timeout: Option<Duration>,
    ) -> Result<CommandOutput, CommandError> {
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            workdir.join(path)
        };
        let read_future = fs_err::tokio::read(&path);
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

        Ok(String::from_utf8(output)
            .context("Failed to parse read file output")?
            .into())
    }

    async fn exec_write_file(
        &self,
        workdir: &Path,
        path: &Path,
        content: &str,
        timeout: Option<Duration>,
    ) -> Result<CommandOutput, CommandError> {
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            workdir.join(path)
        };
        if let Some(parent) = path.parent() {
            let _ = fs_err::tokio::create_dir_all(parent).await;
        }
        let write_future = fs_err::tokio::write(&path, content);
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

    async fn collect_process_output(
        stdout_task: Option<JoinHandle<Vec<String>>>,
        stderr_task: Option<JoinHandle<Vec<String>>>,
    ) -> (Vec<String>, Vec<String>) {
        let stdout = match stdout_task {
            Some(task) => match task.await {
                Ok(lines) => lines,
                Err(err) => {
                    tracing::warn!(?err, "failed to collect stdout from command");
                    Vec::new()
                }
            },
            None => Vec::new(),
        };

        let stderr = match stderr_task {
            Some(task) => match task.await {
                Ok(lines) => lines,
                Err(err) => {
                    tracing::warn!(?err, "failed to collect stderr from command");
                    Vec::new()
                }
            },
            None => Vec::new(),
        };

        (stdout, stderr)
    }

    fn merge_output(stdout: &[String], stderr: &[String]) -> CommandOutput {
        stdout
            .iter()
            .chain(stderr.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
            .into()
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
    use futures_util::StreamExt as _;
    use indoc::indoc;
    use std::{path::Path, sync::Arc, time::Duration};
    use swiftide_core::{Command, ExecutorExt, ToolExecutor};
    use temp_dir::TempDir;

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
        assert_eq!(output.to_string(), format!("{file_content}"));

        let output = executor
            .exec_cmd(&Command::read_file(&file_path))
            .await
            .unwrap();
        assert_eq!(output.to_string(), format!("{file_content}\n"));

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
        assert_eq!(output.to_string().trim(), "hello world");

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
                assert!(output.to_string().contains("ready"));
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
                assert!(output.to_string().is_empty());
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
        let output = executor.exec_cmd(&echo_cmd).await?.to_string();

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
        let output = executor.exec_cmd(&echo_cmd).await?.to_string();

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
        let output = executor.exec_cmd(&echo_cmd).await?.to_string();

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
        let output = executor
            .exec_cmd(&Command::shell(script))
            .await?
            .to_string();

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
        assert_eq!(output.to_string(), format!("{file_content}"));

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
            assert!(matches!(err, CommandError::NonZeroExit(..)));
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
        let output = executor.exec_cmd(&read_cmd).await?.output;

        // Verify that the content read from the file matches the expected content
        assert_eq!(output, file_content);

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
        let pwd_output = executor.exec_cmd(&pwd_cmd).await?.to_string();
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
        let read_output = executor.exec_cmd(&read_cmd).await?.to_string();
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
        let pwd_output = executor.exec_cmd(&pwd_cmd).await?.to_string();
        let pwd_path = std::fs::canonicalize(pwd_output.trim())?;
        assert_eq!(pwd_path, std::fs::canonicalize(&nested_dir)?);

        let mut write_cmd = Command::write_file("file.txt", "hello");
        write_cmd.current_dir(Path::new("nested"));
        executor.exec_cmd(&write_cmd).await?;

        assert!(!base_path.join("file.txt").exists());
        assert!(nested_dir.join("file.txt").exists());

        let mut read_cmd = Command::read_file("file.txt");
        read_cmd.current_dir(Path::new("nested"));
        let read_output = executor.exec_cmd(&read_cmd).await?.to_string();
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
