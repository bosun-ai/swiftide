//! Local executor for running tools on the local machine.
//!
//! By default will use the current directory as the working directory.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
};

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use derive_builder::Builder;
use swiftide_core::{Command, CommandError, CommandOutput, Loader, ToolExecutor};
use swiftide_indexing::loaders::FileLoader;
use tokio::{
    io::{AsyncBufReadExt as _, AsyncWriteExt as _},
    task::JoinSet,
};

#[derive(Debug, Clone, Builder)]
pub struct LocalExecutor {
    #[builder(default = ".".into(), setter(into))]
    workdir: PathBuf,

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
            env_clear: false,
            env_remove: Vec::new(),
            envs: HashMap::new(),
        }
    }

    pub fn builder() -> LocalExecutorBuilder {
        LocalExecutorBuilder::default()
    }

    #[allow(clippy::too_many_lines)]
    async fn exec_shell(&self, cmd: &str) -> Result<CommandOutput, CommandError> {
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
                .current_dir(&self.workdir)
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
            command.arg("-c").arg(cmd).current_dir(&self.workdir);

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
                .current_dir(&self.workdir)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?
        };
        // Run the command in a shell

        let mut joinset = JoinSet::new();

        if let Some(stdout) = child.stdout.take() {
            joinset.spawn(async move {
                let mut lines = tokio::io::BufReader::new(stdout).lines();
                let mut out = Vec::new();
                while let Ok(Some(line)) = lines.next_line().await {
                    out.push(line);
                }
                out
            });
        } else {
            tracing::warn!("Command has no stdout");
        }

        if let Some(stderr) = child.stderr.take() {
            joinset.spawn(async move {
                let mut lines = tokio::io::BufReader::new(stderr).lines();
                let mut out = Vec::new();
                while let Ok(Some(line)) = lines.next_line().await {
                    out.push(line);
                }
                out
            });
        } else {
            tracing::warn!("Command has no stderr");
        }

        let outputs = joinset.join_all().await;
        let &[stdout, stderr] = outputs
            .iter()
            .map(Vec::as_slice)
            .collect::<Vec<_>>()
            .as_slice()
        else {
            // This should never happen
            return Err(anyhow::anyhow!("Failed to get outputs from command").into());
        };

        // outputs stdout and stderr should be empty
        let output = child
            .wait_with_output()
            .await
            .map_err(anyhow::Error::from)?;

        let cmd_output = stdout
            .iter()
            .chain(stderr.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
            .into();

        if output.status.success() {
            Ok(cmd_output)
        } else {
            Err(CommandError::NonZeroExit(cmd_output))
        }
    }

    async fn exec_read_file(&self, path: &Path) -> Result<CommandOutput, CommandError> {
        let path = self.workdir.join(path);
        let output = fs_err::tokio::read(&path).await?;

        Ok(String::from_utf8(output)
            .context("Failed to parse read file output")?
            .into())
    }

    async fn exec_write_file(
        &self,
        path: &Path,
        content: &str,
    ) -> Result<CommandOutput, CommandError> {
        let path = self.workdir.join(path);
        if let Some(parent) = path.parent() {
            let _ = fs_err::tokio::create_dir_all(parent).await;
        }
        fs_err::tokio::write(&path, content).await?;

        Ok(CommandOutput::empty())
    }
}
#[async_trait]
impl ToolExecutor for LocalExecutor {
    /// Execute a `Command` on the local machine
    #[tracing::instrument(skip_self)]
    async fn exec_cmd(&self, cmd: &Command) -> Result<swiftide_core::CommandOutput, CommandError> {
        match cmd {
            Command::Shell(cmd) => __self.exec_shell(cmd).await,
            Command::ReadFile(path) => __self.exec_read_file(path).await,
            Command::WriteFile(path, content) => __self.exec_write_file(path, content).await,
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
    use swiftide_core::{Command, ToolExecutor};
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
            Command::Shell(format!("echo '{}' > {}", file_content, file_path.display()));

        // Execute the write command
        executor.exec_cmd(&write_cmd).await?;

        // Verify that the file was created successfully
        assert!(file_path.exists());

        // Write a shell command to read the file's content
        let read_cmd = Command::Shell(format!("cat {}", file_path.display()));

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
        let echo_cmd = Command::Shell("echo 'hello world'".to_string());

        // Execute the echo command
        let output = executor.exec_cmd(&echo_cmd).await?;

        // Verify that the output matches the expected content
        assert_eq!(output.to_string().trim(), "hello world");

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
        let echo_cmd = Command::Shell("printenv".to_string());

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
        let echo_cmd = Command::Shell("printenv".to_string());

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
        let echo_cmd = Command::Shell("printenv".to_string());

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
        let write_cmd = Command::Shell(format!("echo '{file_content}' > {file_path}"));

        // Execute the write command
        executor.exec_cmd(&write_cmd).await?;

        // Write a shell command to read the file's content
        let read_cmd = Command::Shell(format!("cat {file_path}"));

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
        let cmd = Command::ReadFile(file_path.clone());
        let result = executor.exec_cmd(&cmd).await;

        if let Err(err) = result {
            assert!(matches!(err, CommandError::NonZeroExit(..)));
        } else {
            panic!("Expected error but got {result:?}");
        }

        // Create a write command
        let write_cmd = Command::WriteFile(file_path.clone(), file_content.to_string());

        // Execute the write command
        executor.exec_cmd(&write_cmd).await?;

        // Verify that the file was created successfully
        assert!(file_path.exists());

        // Create a read command
        let read_cmd = Command::ReadFile(file_path.clone());

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
        let pwd_cmd = Command::Shell("pwd".to_string());
        let pwd_output = executor.exec_cmd(&pwd_cmd).await?.to_string();
        let pwd_path = std::fs::canonicalize(pwd_output.trim())?;
        let temp_path = std::fs::canonicalize(temp_path)?;
        assert_eq!(pwd_path, temp_path);

        // 3. Write a file using WriteFile (should land in workdir)
        let fname = "workdir_check.txt";
        let write_cmd = Command::WriteFile(fname.into(), "test123".into());
        executor.exec_cmd(&write_cmd).await?;

        // 4. Assert file exists in workdir, not current dir
        let expected_path = temp_path.join(fname);
        assert!(expected_path.exists());
        assert!(!Path::new(fname).exists());

        // 5. Write/read using ReadFile
        let read_cmd = Command::ReadFile(fname.into());
        let read_output = executor.exec_cmd(&read_cmd).await?.to_string();
        assert_eq!(read_output.trim(), "test123");

        // 6. Clean up
        fs::remove_file(&expected_path)?;

        Ok(())
    }
}
