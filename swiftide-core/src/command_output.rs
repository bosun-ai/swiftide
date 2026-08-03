use std::borrow::Cow;

use bytes::Bytes;

/// A chunk observed while reading a command's output pipes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutputChunk {
    /// Bytes read from stdout.
    Stdout(Bytes),
    /// Bytes read from stderr.
    Stderr(Bytes),
}

impl CommandOutputChunk {
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Stdout(bytes) | Self::Stderr(bytes) => bytes,
        }
    }

    pub fn into_bytes(self) -> Bytes {
        match self {
            Self::Stdout(bytes) | Self::Stderr(bytes) => bytes,
        }
    }

    pub fn is_stdout(&self) -> bool {
        matches!(self, Self::Stdout(_))
    }

    pub fn is_stderr(&self) -> bool {
        matches!(self, Self::Stderr(_))
    }
}

/// Output collected from a finished command.
///
/// Chunks retain the order in which the executor observed stdout and stderr. This is not a
/// guarantee of the order in which the process wrote to separate operating-system pipes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandOutput {
    chunks: Vec<CommandOutputChunk>,
}

impl CommandOutput {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Creates output containing one stdout chunk.
    pub fn new(stdout: impl Into<Vec<u8>>) -> Self {
        Self::from_chunks([CommandOutputChunk::Stdout(Bytes::from(stdout.into()))])
    }

    /// Creates output containing stdout followed by stderr.
    pub fn from_parts(stdout: impl Into<Vec<u8>>, stderr: impl Into<Vec<u8>>) -> Self {
        Self::from_chunks([
            CommandOutputChunk::Stdout(Bytes::from(stdout.into())),
            CommandOutputChunk::Stderr(Bytes::from(stderr.into())),
        ])
    }

    pub fn from_chunks(chunks: impl IntoIterator<Item = CommandOutputChunk>) -> Self {
        Self {
            chunks: chunks
                .into_iter()
                .filter(|chunk| !chunk.as_bytes().is_empty())
                .collect(),
        }
    }

    pub fn chunks(&self) -> &[CommandOutputChunk] {
        &self.chunks
    }

    pub fn into_chunks(self) -> Vec<CommandOutputChunk> {
        self.chunks
    }

    /// Converts all output to text in observed chunk order.
    pub fn to_string_lossy(&self) -> Cow<'_, str> {
        self.selected_to_string_lossy(None)
    }

    pub fn stdout_to_string_lossy(&self) -> Cow<'_, str> {
        self.selected_to_string_lossy(Some(OutputKind::Stdout))
    }

    pub fn stderr_to_string_lossy(&self) -> Cow<'_, str> {
        self.selected_to_string_lossy(Some(OutputKind::Stderr))
    }

    /// Converts all output to owned text in observed chunk order.
    pub fn into_string_lossy(self) -> String {
        bytes_into_string_lossy(self.into_selected_bytes(None))
    }

    pub fn into_stdout_string_lossy(self) -> String {
        bytes_into_string_lossy(self.into_selected_bytes(Some(OutputKind::Stdout)))
    }

    pub fn into_stderr_string_lossy(self) -> String {
        bytes_into_string_lossy(self.into_selected_bytes(Some(OutputKind::Stderr)))
    }

    pub fn len(&self) -> usize {
        self.chunks
            .iter()
            .map(CommandOutputChunk::as_bytes)
            .map(<[u8]>::len)
            .sum()
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    fn selected_to_string_lossy(&self, kind: Option<OutputKind>) -> Cow<'_, str> {
        let mut chunks = self.selected_chunks(kind);
        let Some(first) = chunks.next() else {
            return Cow::Borrowed("");
        };
        if chunks.next().is_none() {
            return String::from_utf8_lossy(first.as_bytes());
        }

        let mut bytes = Vec::with_capacity(
            self.selected_chunks(kind)
                .map(CommandOutputChunk::as_bytes)
                .map(<[u8]>::len)
                .sum(),
        );
        for chunk in self.selected_chunks(kind) {
            bytes.extend_from_slice(chunk.as_bytes());
        }
        Cow::Owned(bytes_into_string_lossy(bytes))
    }

    fn selected_chunks(
        &self,
        kind: Option<OutputKind>,
    ) -> impl Iterator<Item = &CommandOutputChunk> {
        self.chunks
            .iter()
            .filter(move |chunk| kind.is_none_or(|kind| kind.matches(chunk)))
    }

    fn into_selected_bytes(self, kind: Option<OutputKind>) -> Vec<u8> {
        let selected_len = self
            .chunks
            .iter()
            .filter(|chunk| kind.is_none_or(|kind| kind.matches(chunk)))
            .map(CommandOutputChunk::as_bytes)
            .map(<[u8]>::len)
            .sum::<usize>();
        let mut chunks = self
            .chunks
            .into_iter()
            .filter(|chunk| kind.is_none_or(|kind| kind.matches(chunk)));
        let Some(first) = chunks.next() else {
            return Vec::new();
        };

        let mut bytes = Vec::from(first.into_bytes());
        bytes.reserve(selected_len - bytes.len());
        for chunk in chunks {
            bytes.extend_from_slice(&chunk.into_bytes());
        }
        bytes
    }
}

impl<T: Into<Vec<u8>>> From<T> for CommandOutput {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Display for CommandOutput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.to_string_lossy())
    }
}

#[derive(Clone, Copy)]
enum OutputKind {
    Stdout,
    Stderr,
}

impl OutputKind {
    fn matches(self, chunk: &CommandOutputChunk) -> bool {
        match self {
            Self::Stdout => chunk.is_stdout(),
            Self::Stderr => chunk.is_stderr(),
        }
    }
}

fn bytes_into_string_lossy(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes)
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
}

#[cfg(test)]
mod tests {
    use super::{CommandOutput, CommandOutputChunk};
    use bytes::Bytes;
    use std::borrow::Cow;

    #[test]
    fn retains_observed_chunk_order() {
        let output = output([stdout("one"), stderr("two"), stdout("three")]);

        assert_eq!(output.to_string_lossy(), "onetwothree");
        assert_eq!(output.stdout_to_string_lossy(), "onethree");
        assert_eq!(output.stderr_to_string_lossy(), "two");
        assert_eq!(output.len(), 11);
    }

    #[test]
    fn exposes_raw_chunks() {
        let output = output([stdout("out"), stderr("err")]);

        assert_eq!(output.chunks(), [stdout("out"), stderr("err")]);
    }

    #[test]
    fn borrows_one_valid_chunk() {
        let output = CommandOutput::new("hello");

        assert!(matches!(output.to_string_lossy(), Cow::Borrowed("hello")));
        assert!(matches!(
            output.stdout_to_string_lossy(),
            Cow::Borrowed("hello")
        ));
    }

    #[test]
    fn decodes_utf8_split_across_chunks() {
        let output = output([
            stdout(Bytes::from_static(&[0xf0, 0x9f])),
            stdout(Bytes::from_static(&[0x98, 0x80])),
        ]);

        assert_eq!(output.to_string_lossy(), "😀");
        assert_eq!(output.stdout_to_string_lossy(), "😀");
    }

    #[test]
    fn replaces_invalid_utf8_only_when_text_is_requested() {
        let output = output([stdout(Bytes::from_static(b"valid\xff"))]);

        assert_eq!(output.chunks()[0].as_bytes(), b"valid\xff");
        assert!(matches!(output.to_string_lossy(), Cow::Owned(_)));
    }

    #[test]
    fn consuming_helpers_filter_and_preserve_order() {
        let output = output([stdout("one"), stderr("two"), stdout("three")]);
        assert_eq!(output.clone().into_string_lossy(), "onetwothree");
        assert_eq!(output.clone().into_stdout_string_lossy(), "onethree");
        assert_eq!(output.into_stderr_string_lossy(), "two");
    }

    #[test]
    fn display_uses_observed_chunk_order() {
        assert_eq!(output([stdout("out"), stderr("err")]).to_string(), "outerr");
    }

    #[test]
    fn omits_empty_chunks() {
        let output = CommandOutput::from_parts("", "");

        assert!(output.is_empty());
        assert!(output.chunks().is_empty());
    }

    fn output(chunks: impl IntoIterator<Item = CommandOutputChunk>) -> CommandOutput {
        CommandOutput::from_chunks(chunks)
    }

    fn stdout(bytes: impl Into<Bytes>) -> CommandOutputChunk {
        CommandOutputChunk::Stdout(bytes.into())
    }

    fn stderr(bytes: impl Into<Bytes>) -> CommandOutputChunk {
        CommandOutputChunk::Stderr(bytes.into())
    }
}
