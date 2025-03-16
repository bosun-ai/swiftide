//! Local executor for running tools on the local machine.
//!
//! By default will use the current directory as the working directory.
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use derive_builder::Builder;
use swiftide_core::{Command, CommandError, CommandOutput, ToolExecutor};

#[derive(Debug, Clone, Builder)]
pub struct LocalExecutor {
    #[builder(default = ".".into(), setter(into))]
    workdir: PathBuf,
}

impl Default for LocalExecutor {
    fn default() -> Self {
        LocalExecutor {
            workdir: ".".into(),
        }
    }
}

impl LocalExecutor {
    pub fn new(workdir: impl Into<PathBuf>) -> Self {
        LocalExecutor {
            workdir: workdir.into(),
        }
    }

    pub fn builder() -> LocalExecutorBuilder {
        LocalExecutorBuilder::default()
    }

    async fn exec_shell(&self, cmd: &str) -> Result<CommandOutput, CommandError> {
        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(&self.workdir)
            .output()
            .await
            .context("Executor could not run command")?;

        let stdout = String::from_utf8(output.stdout).context("Failed to parse stdout")?;
        let stderr = String::from_utf8(output.stderr).context("Failed to parse stderr")?;
        let merged_output = format!("{stdout}{stderr}");

        if output.status.success() {
            Ok(merged_output.into())
        } else {
            Err(CommandError::NonZeroExit(merged_output.into()))
        }
    }

    async fn exec_read_file(&self, path: &Path) -> Result<CommandOutput, CommandError> {
        let output = fs_err::tokio::read(path).await?;

        Ok(String::from_utf8(output)
            .context("Failed to parse read file output")?
            .into())
    }

    async fn exec_write_file(
        &self,
        path: &Path,
        content: &str,
    ) -> Result<CommandOutput, CommandError> {
        if let Some(parent) = path.parent() {
            let _ = fs_err::tokio::create_dir_all(parent).await;
        }
        fs_err::tokio::write(path, content).await?;

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
}

#[cfg(test)]
mod tests {
    use super::*;
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
    async fn test_local_executor_multiline_with_quotes() -> anyhow::Result<()> {
        // Create a temporary directory
        let temp_dir = TempDir::new()?;
        let temp_path = temp_dir.path();

        // Instantiate LocalExecutor with the temporary directory as workdir
        let executor = LocalExecutor {
            workdir: temp_path.to_path_buf(),
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
        assert_eq!(output.to_string(), format!("{file_content}\n"));

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
}
