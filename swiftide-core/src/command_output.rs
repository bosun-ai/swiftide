//! Command output that retains stdout and stderr identity in observed order.

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

    fn into_bytes(self) -> Bytes {
        match self {
            Self::Stdout(bytes) | Self::Stderr(bytes) => bytes,
        }
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

    /// Iterates over borrowed stdout chunks in observed order.
    pub fn stdout(&self) -> impl Iterator<Item = &[u8]> {
        self.chunks.iter().filter_map(|chunk| match chunk {
            CommandOutputChunk::Stdout(bytes) => Some(bytes.as_ref()),
            CommandOutputChunk::Stderr(_) => None,
        })
    }

    /// Iterates over borrowed stderr chunks in observed order.
    pub fn stderr(&self) -> impl Iterator<Item = &[u8]> {
        self.chunks.iter().filter_map(|chunk| match chunk {
            CommandOutputChunk::Stdout(_) => None,
            CommandOutputChunk::Stderr(bytes) => Some(bytes.as_ref()),
        })
    }

    /// Converts all output to owned text in observed chunk order.
    pub fn to_string_lossy(&self) -> String {
        bytes_into_string_lossy(self.bytes())
    }

    /// Returns all output bytes in observed chunk order.
    pub fn into_bytes(self) -> Vec<u8> {
        let output_len = self.len();
        let mut chunks = self.chunks.into_iter();
        let Some(first) = chunks.next() else {
            return Vec::new();
        };

        let mut bytes = Vec::from(first.into_bytes());
        bytes.reserve(output_len - bytes.len());
        for chunk in chunks {
            bytes.extend_from_slice(&chunk.into_bytes());
        }
        bytes
    }

    /// Converts all output to owned text in observed chunk order.
    pub fn into_string_lossy(self) -> String {
        bytes_into_string_lossy(self.into_bytes())
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

    fn bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.len());
        for chunk in &self.chunks {
            bytes.extend_from_slice(chunk.as_bytes());
        }
        bytes
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

fn bytes_into_string_lossy(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes)
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
}

#[cfg(test)]
mod tests {
    use super::{CommandOutput, CommandOutputChunk};
    use bytes::Bytes;

    #[test]
    fn exposes_borrowed_stream_chunks_in_observed_order() {
        let output = output([stdout("one"), stderr("two"), stdout("three")]);
        let stdout = output.stdout().collect::<Vec<_>>();
        let stderr = output.stderr().collect::<Vec<_>>();

        assert_eq!(stdout, [b"one".as_slice(), b"three".as_slice()]);
        assert_eq!(stderr, [b"two".as_slice()]);
        assert_eq!(stdout[0].as_ptr(), output.chunks()[0].as_bytes().as_ptr());
        assert_eq!(stderr[0].as_ptr(), output.chunks()[1].as_bytes().as_ptr());
    }

    #[test]
    fn consumes_output_in_observed_order() {
        let output = output([stdout("one"), stderr("two"), stdout("three")]);

        assert_eq!(output.into_string_lossy(), "onetwothree");
    }

    #[test]
    fn reads_output_as_owned_text_in_observed_order() {
        let output = output([stdout("one"), stderr("two"), stdout("three")]);

        assert_eq!(output.to_string_lossy(), "onetwothree");
    }

    #[test]
    fn consuming_one_chunk_reuses_its_allocation() {
        let bytes = b"hello".to_vec();
        let pointer = bytes.as_ptr();
        let output = CommandOutput::new(bytes);

        let bytes = output.into_bytes();
        assert_eq!(bytes.as_ptr(), pointer);
    }

    #[test]
    fn decodes_utf8_split_across_chunks() {
        let output = output([
            stdout(Bytes::from_static(&[0xf0, 0x9f])),
            stdout(Bytes::from_static(&[0x98, 0x80])),
        ]);

        assert_eq!(output.into_string_lossy(), "😀");
    }

    #[test]
    fn replaces_invalid_utf8_only_when_text_is_requested() {
        let output = output([stdout(Bytes::from_static(b"valid\xff"))]);

        assert_eq!(output.into_string_lossy(), "valid\u{fffd}");
    }

    #[test]
    fn display_uses_observed_chunk_order() {
        let output = output([stdout("out"), stderr("err")]);

        assert_eq!(output.to_string(), "outerr");
    }

    #[test]
    fn display_decodes_utf8_split_across_chunks() {
        let output = output([
            stdout(Bytes::from_static(&[0xf0, 0x9f])),
            stdout(Bytes::from_static(&[0x98, 0x80])),
        ]);

        assert_eq!(output.to_string(), "😀");
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
