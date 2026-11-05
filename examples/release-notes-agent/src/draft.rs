//! Live preview of release notes while the model is still writing them.
//!
//! The notes arrive as the `markdown` argument of a streamed `write_changelog` tool call, i.e. as a
//! growing, JSON encoded string like `{"markdown":"## 0.34.0\n\n### Hig`. [`DraftPreview`] decodes
//! the part of that string that is complete so far and hands back only what is new.

/// Decodes the `markdown` argument of one streamed tool call, chunk by chunk.
#[derive(Debug, Default)]
pub struct DraftPreview {
    call_id: Option<String>,
    /// Byte offset into the raw arguments up to which the draft has been decoded.
    decoded_until: Option<usize>,
    finished: bool,
    ends_with_newline: bool,
}

impl DraftPreview {
    /// Takes the accumulated raw arguments of tool call `call_id` and returns the newly
    /// completed part of the draft, if any.
    ///
    /// Once the draft is complete it is terminated with a newline, so whatever is printed next
    /// starts on a line of its own.
    pub fn advance(&mut self, call_id: &str, raw_args: &str) -> Option<String> {
        if self.call_id.as_deref() != Some(call_id) {
            *self = Self {
                call_id: Some(call_id.to_owned()),
                ..Self::default()
            };
        }
        if self.finished {
            return None;
        }

        let start = match self.decoded_until {
            Some(offset) => offset,
            None => markdown_value_start(raw_args)?,
        };
        let pending = &raw_args[start..];
        let (complete, closed) = complete_json_prefix(pending);
        self.decoded_until = Some(start + complete);
        self.finished = closed;

        let mut text = if complete == 0 {
            String::new()
        } else {
            serde_json::from_str::<String>(&format!("\"{}\"", &pending[..complete])).ok()?
        };
        if let Some(last) = text.chars().last() {
            self.ends_with_newline = last == '\n';
        }
        if closed && !self.ends_with_newline {
            text.push('\n');
        }
        (!text.is_empty()).then_some(text)
    }
}

/// Byte offset just after the opening quote of the `markdown` value.
fn markdown_value_start(raw_args: &str) -> Option<usize> {
    const KEY: &str = "\"markdown\"";
    let after_key = raw_args.find(KEY)? + KEY.len();
    let rest = &raw_args[after_key..];
    let after_colon = rest.trim_start().strip_prefix(':')?;
    let after_quote = after_colon.trim_start().strip_prefix('"')?;
    Some(raw_args.len() - after_quote.len())
}

/// Length of the longest prefix of a JSON string body that does not end inside an escape
/// sequence, and whether the closing quote was reached.
fn complete_json_prefix(body: &str) -> (usize, bool) {
    let bytes = body.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let step = match bytes[index] {
            b'"' => return (index, true),
            b'\\' => match bytes.get(index + 1) {
                None => break,
                Some(b'u') => match unicode_escape_len(&body[index..]) {
                    Some(len) => len,
                    None => break,
                },
                Some(_) => 2,
            },
            _ => 1,
        };
        index += step;
    }
    (index.min(bytes.len()), false)
}

/// Length of a complete `\uXXXX` escape, or of a complete surrogate pair. `None` while incomplete.
fn unicode_escape_len(escape: &str) -> Option<usize> {
    let code = u16::from_str_radix(escape.get(2..6)?, 16).ok()?;
    if (0xD800..0xDC00).contains(&code) {
        // A high surrogate only decodes together with the low surrogate that follows it.
        escape.get(6..12)?;
        Some(12)
    } else {
        Some(6)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(raw: &str, chunk_size: usize) -> String {
        let mut preview = DraftPreview::default();
        let mut shown = String::new();
        let mut end = 0;
        while end < raw.len() {
            end = (end + chunk_size).min(raw.len());
            while !raw.is_char_boundary(end) {
                end += 1;
            }
            if let Some(text) = preview.advance("call_1", &raw[..end]) {
                shown.push_str(&text);
            }
        }
        shown
    }

    #[test]
    fn decodes_the_draft_regardless_of_chunking() {
        let markdown = "## 0.34.0 \"Rocket\" 🚀\n\n- tabs\tand \\ slashes\n- ünïcode";
        let raw = serde_json::json!({ "markdown": markdown }).to_string();
        let ascii_escaped = raw.replace('🚀', "\\ud83d\\ude80");

        for chunk_size in 1..12 {
            let expected = format!("{markdown}\n");
            assert_eq!(
                stream(&raw, chunk_size),
                expected,
                "chunk size {chunk_size}"
            );
            assert_eq!(stream(&ascii_escaped, chunk_size), expected);
        }
    }

    #[test]
    fn restarts_for_a_new_tool_call() {
        let mut preview = DraftPreview::default();
        assert_eq!(
            preview.advance("a", r#"{"markdown":"one\n"}"#).as_deref(),
            Some("one\n")
        );
        assert_eq!(preview.advance("a", r#"{"markdown":"one\n"}"#), None);
        assert_eq!(
            preview.advance("b", r#"{"markdown": "two"#).as_deref(),
            Some("two")
        );
    }
}
