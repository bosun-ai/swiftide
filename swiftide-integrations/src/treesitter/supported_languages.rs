//! This module defines the supported programming languages for the Swiftide project and provides utility functions
//! for mapping these languages to their respective file extensions and tree-sitter language objects.
//!
//! The primary purpose of this module is to facilitate the recognition and handling of different programming languages
//! by mapping file extensions and converting language enums to tree-sitter language objects for accurate parsing and syntax analysis.
//!
//! # Supported Languages
//! - Rust
//! - Typescript
//! - Python
//! - Ruby
//! - Javascript

#[allow(unused_imports)]
pub use std::str::FromStr as _;

/// Enum representing the supported programming languages in the Swiftide project.
///
/// This enum is used to map programming languages to their respective file extensions and tree-sitter language objects.
/// The `EnumString` and `Display` macros from the `strum_macros` crate are used to provide string conversion capabilities.
/// The `ascii_case_insensitive` attribute allows for case-insensitive string matching.
#[derive(Debug, PartialEq, Clone, Copy, strum_macros::EnumString, strum_macros::Display)]
#[strum(ascii_case_insensitive)]
pub enum SupportedLanguages {
    Rust,
    Typescript,
    Python,
    Ruby,
    Javascript,
}

/// Static array of file extensions for Rust files.
static RUST_EXTENSIONS: &[&str] = &["rs"];

/// Static array of file extensions for Typescript files.
static TYPESCRIPT_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx"];

/// Static array of file extensions for Python files.
static PYTHON_EXTENSIONS: &[&str] = &["py"];

/// Static array of file extensions for Ruby files.
static RUBY_EXTENSIONS: &[&str] = &["rb"];

/// Static array of file extensions for Javascript files.
static JAVASCRIPT_EXTENSIONS: &[&str] = &["js", "jsx"];

impl SupportedLanguages {
    /// Returns the file extensions associated with the supported language.
    ///
    /// # Returns
    /// A static slice of string slices representing the file extensions.
    pub fn file_extensions(&self) -> &[&str] {
        match self {
            SupportedLanguages::Rust => RUST_EXTENSIONS,
            SupportedLanguages::Typescript => TYPESCRIPT_EXTENSIONS,
            SupportedLanguages::Python => PYTHON_EXTENSIONS,
            SupportedLanguages::Ruby => RUBY_EXTENSIONS,
            SupportedLanguages::Javascript => JAVASCRIPT_EXTENSIONS,
        }
    }
}

impl From<SupportedLanguages> for tree_sitter::Language {
    /// Converts a `SupportedLanguages` enum to a `tree_sitter::Language` object.
    ///
    /// This implementation allows for the conversion of the supported languages to their respective tree-sitter language objects,
    /// enabling accurate parsing and syntax analysis.
    ///
    /// # Parameters
    /// - `val`: The `SupportedLanguages` enum value to be converted.
    ///
    /// # Returns
    /// A `tree_sitter::Language` object corresponding to the provided `SupportedLanguages` enum value.
    fn from(val: SupportedLanguages) -> Self {
        match val {
            SupportedLanguages::Rust => tree_sitter_rust::LANGUAGE,
            SupportedLanguages::Python => tree_sitter_python::LANGUAGE,
            SupportedLanguages::Typescript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            SupportedLanguages::Javascript => tree_sitter_javascript::LANGUAGE,
            SupportedLanguages::Ruby => tree_sitter_ruby::LANGUAGE,
        }
        .into()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// Tests the case-insensitive string conversion for `SupportedLanguages`.
    #[test]
    fn test_supported_languages_from_str() {
        assert_eq!(
            SupportedLanguages::from_str("rust"),
            Ok(SupportedLanguages::Rust)
        );
        assert_eq!(
            SupportedLanguages::from_str("typescript"),
            Ok(SupportedLanguages::Typescript)
        );
    }

    /// Tests the case-insensitive string conversion for `SupportedLanguages` with different casing.
    #[test]
    fn test_supported_languages_from_str_case_insensitive() {
        assert_eq!(
            SupportedLanguages::from_str("Rust"),
            Ok(SupportedLanguages::Rust)
        );
        assert_eq!(
            SupportedLanguages::from_str("TypeScript"),
            Ok(SupportedLanguages::Typescript)
        );
    }
}
