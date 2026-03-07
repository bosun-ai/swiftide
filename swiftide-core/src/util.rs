//! Utility functions for Swiftide

/// Safely truncates a string to a maximum number of characters.
///
/// Respects utf8 character boundaries.
pub fn safe_truncate_utf8(s: impl AsRef<str>, max_chars: usize) -> String {
    s.as_ref().chars().take(max_chars).collect()
}

/// Debug print a long string by truncating to n characters
///
/// Enabled with the `truncate-debug` feature flag, which is enabled by default.
///
/// If debugging large outputs is needed, set `swiftide_core` to `no-default-features`
///
/// # Example
///
/// ```ignore
/// # use swiftide_core::util::debug_long_utf8;
/// let s = debug_long_utf8("🦀".repeat(10), 3);
///
/// assert_eq!(s, "🦀🦀🦀 (10)");
/// ```
pub fn debug_long_utf8(s: impl AsRef<str>, max_chars: usize) -> String {
    if cfg!(feature = "truncate-debug") {
        let trunc = safe_truncate_utf8(&s, max_chars);

        format!("{} ({})", trunc, s.as_ref().chars().count())
    } else {
        s.as_ref().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_truncate_str_with_utf8_char_boundary() {
        let s = "🦀".repeat(101);

        // Single char
        assert_eq!(safe_truncate_utf8(&s, 100).chars().count(), 100);

        // With invalid char boundary
        let s = "Jürgen".repeat(100);
        assert_eq!(safe_truncate_utf8(&s, 100).chars().count(), 100);
    }
}
