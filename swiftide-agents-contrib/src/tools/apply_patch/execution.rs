use super::{
    matching::apply_chunks_to_contents,
    parser::{Hunk, UpdateFileChunk},
};
use anyhow::anyhow;
use shell_escape::unix::escape;
use std::path::{Component, Path, PathBuf};
use swiftide_core::{Command, chat_completion::errors::ToolError};

enum UpdateOutcome {
    Modified(PathBuf),
    Moved(PathBuf, PathBuf),
    AlreadyPresent(PathBuf),
}

#[derive(Debug)]
pub(super) enum OperationOutcome {
    Applied(String),
    AlreadyPresent(String),
    NotApplied { operation: String, reason: String },
}

pub(super) async fn apply_hunk(
    executor: &dyn swiftide_core::ToolExecutor,
    hunk: Hunk,
) -> OperationOutcome {
    let (path, operation) = match &hunk {
        Hunk::Add { path, .. } => (path, format!("Add: {}", path.display())),
        Hunk::Delete { path } => (path, format!("Delete: {}", path.display())),
        Hunk::Update { path, .. } => (path, format!("Update: {}", path.display())),
    };
    if let Err(error) = validate_patch_path(path) {
        return OperationOutcome::NotApplied {
            operation,
            reason: error.to_string(),
        };
    }
    match execute_hunk(executor, hunk).await {
        Ok(outcome) => outcome,
        Err(error) => OperationOutcome::NotApplied {
            operation,
            reason: error.to_string(),
        },
    }
}

async fn execute_hunk(
    executor: &dyn swiftide_core::ToolExecutor,
    hunk: Hunk,
) -> Result<OperationOutcome, ToolError> {
    match hunk {
        Hunk::Add { path, contents } => {
            apply_add_file(executor, &path, contents).await?;
            Ok(OperationOutcome::Applied(format!(
                "Added: {}",
                path.display()
            )))
        }
        Hunk::Delete { path } => {
            apply_delete_file(executor, &path).await?;
            Ok(OperationOutcome::Applied(format!(
                "Deleted: {}",
                path.display()
            )))
        }
        Hunk::Update {
            path,
            move_path,
            chunks,
        } => {
            if let Some(destination) = &move_path {
                validate_patch_path(destination)?;
                if destination == &path {
                    return Err(ToolError::unknown(anyhow!(
                        "move destination must differ from the source"
                    )));
                }
            }
            let outcome =
                match apply_update_file(executor, &path, move_path.as_ref(), &chunks).await? {
                    UpdateOutcome::Modified(path) => {
                        OperationOutcome::Applied(format!("Modified: {}", path.display()))
                    }
                    UpdateOutcome::Moved(from, to) => OperationOutcome::Applied(format!(
                        "Moved: {} -> {}",
                        from.display(),
                        to.display()
                    )),
                    UpdateOutcome::AlreadyPresent(path) => {
                        OperationOutcome::AlreadyPresent(format!("Modified: {}", path.display()))
                    }
                };
            Ok(outcome)
        }
    }
}

async fn apply_add_file(
    executor: &dyn swiftide_core::ToolExecutor,
    path: &Path,
    contents: String,
) -> Result<(), ToolError> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        let cmd = Command::shell(format!("mkdir -p -- {}", escape(parent.to_string_lossy())));
        executor
            .exec_cmd(&cmd)
            .await
            .map_err(|err| ToolError::unknown(anyhow!(err)))?;
    }

    let command = Command::write_file(path.to_path_buf(), contents);
    executor
        .exec_cmd(&command)
        .await
        .map_err(|err| ToolError::unknown(anyhow!(err)))?;
    Ok(())
}

async fn apply_delete_file(
    executor: &dyn swiftide_core::ToolExecutor,
    path: &Path,
) -> Result<(), ToolError> {
    let read_cmd = Command::read_file(path.to_path_buf());
    executor.exec_cmd(&read_cmd).await.map_err(|err| {
        ToolError::unknown(anyhow!(format!(
            "failed to read {path}: {err}",
            path = path.display()
        )))
    })?;

    let shell_cmd = Command::shell(format!("rm -- {}", escape(path.to_string_lossy())));
    executor.exec_cmd(&shell_cmd).await.map_err(|err| {
        ToolError::unknown(anyhow!(format!(
            "failed to remove {path}: {err}",
            path = path.display()
        )))
    })?;
    Ok(())
}

async fn apply_update_file(
    executor: &dyn swiftide_core::ToolExecutor,
    source: &Path,
    move_path: Option<&PathBuf>,
    chunks: &[UpdateFileChunk],
) -> Result<UpdateOutcome, ToolError> {
    let read_cmd = Command::read_file(source.to_path_buf());
    let original = executor
        .exec_cmd(&read_cmd)
        .await
        .map_err(|err| {
            ToolError::unknown(anyhow!(format!(
                "failed to read {source}: {err}",
                source = source.display()
            )))
        })?
        .into_string_lossy();

    let update = apply_chunks_to_contents(source, &original, chunks)?;

    if let Some(dest) = move_path {
        if let Some(parent) = dest.parent().filter(|p| !p.as_os_str().is_empty()) {
            let mkdir_cmd =
                Command::shell(format!("mkdir -p -- {}", escape(parent.to_string_lossy())));
            executor
                .exec_cmd(&mkdir_cmd)
                .await
                .map_err(|err| ToolError::unknown(anyhow!(err)))?;
        }

        let write_cmd = Command::write_file(dest.clone(), update.contents);
        executor.exec_cmd(&write_cmd).await.map_err(|err| {
            ToolError::unknown(anyhow!(format!(
                "failed to write moved file {dest}: {err}",
                dest = dest.display()
            )))
        })?;

        let remove_cmd = Command::shell(format!("rm -- {}", escape(source.to_string_lossy())));
        executor.exec_cmd(&remove_cmd).await.map_err(|err| {
            ToolError::unknown(anyhow!(format!(
                "destination {dest} was written, but source {source} was not removed: {err}",
                dest = dest.display(),
                source = source.display()
            )))
        })?;

        Ok(UpdateOutcome::Moved(source.to_path_buf(), dest.clone()))
    } else if !update.changed {
        Ok(UpdateOutcome::AlreadyPresent(source.to_path_buf()))
    } else {
        let write_cmd = Command::write_file(source.to_path_buf(), update.contents);
        executor.exec_cmd(&write_cmd).await.map_err(|err| {
            ToolError::unknown(anyhow!(format!(
                "failed to write {source}: {err}",
                source = source.display()
            )))
        })?;

        Ok(UpdateOutcome::Modified(source.to_path_buf()))
    }
}

fn validate_patch_path(path: &Path) -> Result<(), ToolError> {
    if path.is_absolute() {
        return Err(ToolError::unknown(anyhow!(
            "absolute paths are not supported"
        )));
    }

    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(ToolError::unknown(anyhow!(
            "paths cannot contain parent directory components"
        )));
    }

    if path.components().next().is_none() {
        return Err(ToolError::unknown(anyhow!("empty paths are not supported")));
    }

    Ok(())
}
