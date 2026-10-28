use swiftide_core::{CommandError, CommandOutput};

pub(crate) fn accept_non_zero_exit(
    result: Result<CommandOutput, CommandError>,
) -> Result<CommandOutput, CommandError> {
    if let Err(CommandError::NonZeroExit(output)) = result {
        tracing::warn!(?output, "command exited with non-zero status");
        return Ok(output);
    }
    result
}
