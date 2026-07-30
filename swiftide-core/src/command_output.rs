use std::borrow::Cow;

/// The exact bytes observed while executing a command.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandOutput {
    bytes: Vec<u8>,
}

impl CommandOutput {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn to_string_lossy(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.bytes)
    }

    pub fn into_string_lossy(self) -> String {
        String::from_utf8(self.bytes)
            .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
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

#[cfg(test)]
mod tests {
    use super::CommandOutput;
    use std::borrow::Cow;

    #[test]
    fn borrows_valid_text() {
        let output = CommandOutput::new("hello");

        assert!(matches!(output.to_string_lossy(), Cow::Borrowed("hello")));
    }

    #[test]
    fn replaces_invalid_utf8_only_when_text_is_requested() {
        let output = CommandOutput::new(b"valid\xff".as_slice());

        assert_eq!(output.as_bytes(), b"valid\xff");
        assert!(matches!(output.to_string_lossy(), Cow::Owned(_)));
        assert_eq!(output.to_string_lossy(), "valid\u{fffd}");
    }

    #[test]
    fn consuming_valid_text_reuses_the_output_allocation() {
        let bytes = b"hello".to_vec();
        let pointer = bytes.as_ptr();

        let rendered = CommandOutput::new(bytes).into_string_lossy();

        assert_eq!(rendered.as_ptr(), pointer);
    }

    #[test]
    fn preserves_exact_bytes() {
        let bytes = b"one\r\ntwo\n\n".to_vec();

        let output = CommandOutput::new(bytes);

        assert_eq!(output.as_bytes(), b"one\r\ntwo\n\n");
        assert_eq!(output.len(), 10);
    }
}
