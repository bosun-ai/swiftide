use std::borrow::Cow;

/// Output collected from a finished command.
///
/// This follows [`std::process::Output`] for stdout and stderr while command status remains part of
/// [`crate::CommandError`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandOutput {
    /// The data that the command wrote to stdout.
    pub stdout: Vec<u8>,
    /// The data that the command wrote to stderr.
    pub stderr: Vec<u8>,
}

impl CommandOutput {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn new(stdout: impl Into<Vec<u8>>) -> Self {
        Self {
            stdout: stdout.into(),
            stderr: Vec::new(),
        }
    }

    pub fn from_parts(stdout: impl Into<Vec<u8>>, stderr: impl Into<Vec<u8>>) -> Self {
        Self {
            stdout: stdout.into(),
            stderr: stderr.into(),
        }
    }

    pub fn stdout_to_string_lossy(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.stdout)
    }

    pub fn stderr_to_string_lossy(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.stderr)
    }

    pub fn into_stdout_string_lossy(self) -> String {
        bytes_into_string_lossy(self.stdout)
    }

    pub fn into_stderr_string_lossy(self) -> String {
        bytes_into_string_lossy(self.stderr)
    }

    pub fn len(&self) -> usize {
        self.stdout.len() + self.stderr.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stdout.is_empty() && self.stderr.is_empty()
    }
}

impl<T: Into<Vec<u8>>> From<T> for CommandOutput {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Display for CommandOutput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.stdout.is_empty() {
            formatter.write_str(&self.stderr_to_string_lossy())
        } else {
            formatter.write_str(&self.stdout_to_string_lossy())
        }
    }
}

fn bytes_into_string_lossy(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes)
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
}

#[cfg(test)]
mod tests {
    use super::CommandOutput;
    use std::borrow::Cow;

    #[test]
    fn exposes_stdout_and_stderr_as_bytes() {
        let output = CommandOutput::from_parts(b"out".to_vec(), b"err".to_vec());

        assert_eq!(output.stdout, b"out");
        assert_eq!(output.stderr, b"err");
        assert_eq!(output.len(), 6);
    }

    #[test]
    fn borrows_valid_text() {
        let output = CommandOutput::from_parts("hello", "warning");

        assert!(matches!(
            output.stdout_to_string_lossy(),
            Cow::Borrowed("hello")
        ));
        assert!(matches!(
            output.stderr_to_string_lossy(),
            Cow::Borrowed("warning")
        ));
    }

    #[test]
    fn replaces_invalid_utf8_only_when_text_is_requested() {
        let output = CommandOutput::from_parts(b"valid\xff".as_slice(), b"error\xfe".as_slice());

        assert_eq!(output.stdout, b"valid\xff");
        assert_eq!(output.stderr, b"error\xfe");
        assert!(matches!(output.stdout_to_string_lossy(), Cow::Owned(_)));
        assert!(matches!(output.stderr_to_string_lossy(), Cow::Owned(_)));
    }

    #[test]
    fn consuming_valid_stdout_reuses_the_output_allocation() {
        let bytes = b"hello".to_vec();
        let pointer = bytes.as_ptr();

        let rendered = CommandOutput::new(bytes).into_stdout_string_lossy();

        assert_eq!(rendered.as_ptr(), pointer);
    }

    #[test]
    fn display_prefers_stdout_and_falls_back_to_stderr() {
        assert_eq!(CommandOutput::from_parts("out", "err").to_string(), "out");
        assert_eq!(CommandOutput::from_parts("", "err").to_string(), "err");
    }
}
