//! Command output that retains stdout and stderr identity in observed order.

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
    /// Returns the bytes in this chunk.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Stdout(bytes) | Self::Stderr(bytes) => bytes,
        }
    }

    /// Returns the bytes in this chunk without copying them.
    pub fn into_bytes(self) -> Bytes {
        match self {
            Self::Stdout(bytes) | Self::Stderr(bytes) => bytes,
        }
    }

    /// Returns `true` when this chunk came from stdout.
    pub fn is_stdout(&self) -> bool {
        matches!(self, Self::Stdout(_))
    }

    /// Returns `true` when this chunk came from stderr.
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
    /// Creates command output without any chunks.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Creates output containing one stdout chunk.
    pub fn new(stdout: impl Into<Bytes>) -> Self {
        Self::from_chunks([CommandOutputChunk::Stdout(stdout.into())])
    }

    /// Creates output containing stdout followed by stderr.
    pub fn from_parts(stdout: impl Into<Bytes>, stderr: impl Into<Bytes>) -> Self {
        Self::from_chunks([
            CommandOutputChunk::Stdout(stdout.into()),
            CommandOutputChunk::Stderr(stderr.into()),
        ])
    }

    /// Creates output from chunks in the order they were observed.
    ///
    /// Empty chunks are discarded.
    pub fn from_chunks(chunks: impl IntoIterator<Item = CommandOutputChunk>) -> Self {
        Self {
            chunks: chunks
                .into_iter()
                .filter(|chunk| !chunk.as_bytes().is_empty())
                .collect(),
        }
    }

    /// Returns the ordered stdout and stderr chunks.
    pub fn chunks(&self) -> &[CommandOutputChunk] {
        &self.chunks
    }

    /// Returns the ordered stdout and stderr chunks without copying them.
    pub fn into_chunks(self) -> Vec<CommandOutputChunk> {
        self.chunks
    }

    /// Returns all output bytes in observed chunk order.
    ///
    /// The bytes are borrowed when output is empty or contains one chunk. Joining multiple chunks
    /// requires an owned value.
    pub fn as_bytes(&self) -> Cow<'_, [u8]> {
        self.selected_bytes(None)
    }

    /// Converts all output to text in observed chunk order.
    ///
    /// Valid UTF-8 is borrowed when output contains one chunk. Joining multiple chunks or replacing
    /// invalid UTF-8 requires an owned value.
    pub fn to_string_lossy(&self) -> Cow<'_, str> {
        bytes_to_string_lossy(self.as_bytes())
    }

    /// Returns a borrowed view of stdout.
    pub fn stdout(&self) -> CommandOutputView<'_> {
        CommandOutputView::new(self, OutputKind::Stdout)
    }

    /// Returns a borrowed view of stderr.
    pub fn stderr(&self) -> CommandOutputView<'_> {
        CommandOutputView::new(self, OutputKind::Stderr)
    }

    /// Converts all output to owned text in observed chunk order.
    pub fn into_string_lossy(self) -> String {
        bytes_into_string_lossy(self.into_selected_bytes(None))
    }

    /// Converts stdout to owned text in observed chunk order.
    pub fn into_stdout_string_lossy(self) -> String {
        bytes_into_string_lossy(self.into_selected_bytes(Some(OutputKind::Stdout)))
    }

    /// Converts stderr to owned text in observed chunk order.
    pub fn into_stderr_string_lossy(self) -> String {
        bytes_into_string_lossy(self.into_selected_bytes(Some(OutputKind::Stderr)))
    }

    /// Returns the number of bytes across all chunks.
    pub fn len(&self) -> usize {
        self.chunks
            .iter()
            .map(CommandOutputChunk::as_bytes)
            .map(<[u8]>::len)
            .sum()
    }

    /// Returns `true` when there is no stdout or stderr output.
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    fn selected_bytes(&self, kind: Option<OutputKind>) -> Cow<'_, [u8]> {
        let mut chunks = self.selected_chunks(kind);
        let Some(first) = chunks.next() else {
            return Cow::Borrowed(b"");
        };
        if chunks.next().is_none() {
            return Cow::Borrowed(first.as_bytes());
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
        Cow::Owned(bytes)
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

/// A borrowed view of one command output pipe.
///
/// Obtain a view with [`CommandOutput::stdout`] or [`CommandOutput::stderr`]. It references the
/// ordered chunk list and does not create a second stdout or stderr buffer.
#[derive(Debug, Clone, Copy)]
pub struct CommandOutputView<'a> {
    output: &'a CommandOutput,
    kind: OutputKind,
}

impl<'a> CommandOutputView<'a> {
    fn new(output: &'a CommandOutput, kind: OutputKind) -> Self {
        Self { output, kind }
    }

    /// Returns the bytes observed on this output pipe.
    ///
    /// The bytes are borrowed when the selected output is empty or contains one chunk. Joining
    /// multiple chunks requires an owned value.
    pub fn as_bytes(&self) -> Cow<'a, [u8]> {
        self.output.selected_bytes(Some(self.kind))
    }

    /// Converts this output pipe to text.
    ///
    /// Valid UTF-8 is borrowed when the selected output contains one chunk. Joining multiple chunks
    /// or replacing invalid UTF-8 requires an owned value.
    pub fn to_string_lossy(&self) -> Cow<'a, str> {
        bytes_to_string_lossy(self.as_bytes())
    }
}

impl<T: Into<Bytes>> From<T> for CommandOutput {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Display for CommandOutput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.to_string_lossy())
    }
}

#[derive(Debug, Clone, Copy)]
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

fn bytes_to_string_lossy(bytes: Cow<'_, [u8]>) -> Cow<'_, str> {
    match bytes {
        Cow::Borrowed(bytes) => String::from_utf8_lossy(bytes),
        Cow::Owned(bytes) => Cow::Owned(bytes_into_string_lossy(bytes)),
    }
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
        assert_eq!(output.stdout().to_string_lossy(), "onethree");
        assert_eq!(output.stderr().to_string_lossy(), "two");
        assert!(matches!(output.as_bytes(), Cow::Owned(_)));
        assert!(matches!(output.stdout().as_bytes(), Cow::Owned(_)));
        assert!(matches!(output.stderr().as_bytes(), Cow::Borrowed(b"two")));
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

        assert!(matches!(output.as_bytes(), Cow::Borrowed(b"hello")));
        assert!(matches!(
            output.stdout().as_bytes(),
            Cow::Borrowed(b"hello")
        ));
        assert!(matches!(output.stderr().as_bytes(), Cow::Borrowed(b"")));
        assert!(matches!(output.to_string_lossy(), Cow::Borrowed("hello")));
        assert!(matches!(
            output.stdout().to_string_lossy(),
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
        assert_eq!(output.stdout().to_string_lossy(), "😀");
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
