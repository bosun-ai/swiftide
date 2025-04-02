//! This module defines the supported programming languages for the Swiftide project and provides
//! utility functions for mapping these languages to their respective file extensions and
//! tree-sitter language objects.
//!
//! The primary purpose of this module is to facilitate the recognition and handling of different
//! programming languages by mapping file extensions and converting language enums to tree-sitter
//! language objects for accurate parsing and syntax analysis.
//!
//! # Supported Languages
//! - Rust
//! - Typescript
//! - Python
//! - Ruby
//! - Javascript
//! - Solidity

#[allow(unused_imports)]
pub use std::str::FromStr as _;

use serde::{Deserialize, Serialize};

/// Enum representing the supported programming languages in the Swiftide project.
///
/// This enum is used to map programming languages to their respective file extensions and
/// tree-sitter language objects. The `EnumString` and `Display` macros from the `strum_macros`
/// crate are used to provide string conversion capabilities. The `ascii_case_insensitive` attribute
/// allows for case-insensitive string matching.
#[derive(
    Debug,
    PartialEq,
    Clone,
    Copy,
    Deserialize,
    Serialize,
    strum_macros::EnumString,
    strum_macros::Display,
    strum_macros::EnumIter,
)]
#[strum(ascii_case_insensitive)]
#[non_exhaustive]
pub enum SupportedLanguages {
    #[serde(alias = "rust")]
    Rust,
    #[serde(alias = "typescript")]
    Typescript,
    #[serde(alias = "python")]
    Python,
    #[serde(alias = "ruby")]
    Ruby,
    #[serde(alias = "javascript")]
    Javascript,
    #[serde(alias = "java")]
    Java,
    #[serde(alias = "go")]
    Go,
    #[serde(alias = "solidity")]
    Solidity,
    #[serde(alias = "c")]
    C,
    #[serde(alias = "cpp", alias = "c++", alias = "C++", rename = "C++")]
    #[strum(
        serialize = "c++",
        serialize = "cpp",
        serialize = "Cpp",
        to_string = "C++"
    )]
    Cpp,
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

/// Static array of file extensions for Java files.
static JAVA_EXTENSIONS: &[&str] = &["java"];

/// Static array of file extensions for Go files.
static GO_EXTENSIONS: &[&str] = &["go"];

/// Static array of file extensions for Solidity files.
static SOLIDITY_EXTENSIONS: &[&str] = &["sol"];

/// Static array of file extensions for C files.
static C_EXTENSIONS: &[&str] = &["c", "h", "o"];

/// Static array of file extensions for C++ files.
static CPP_EXTENSIONS: &[&str] = &["c", "h", "o", "cc", "cpp"];

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
            SupportedLanguages::Java => JAVA_EXTENSIONS,
            SupportedLanguages::Go => GO_EXTENSIONS,
            SupportedLanguages::Solidity => SOLIDITY_EXTENSIONS,
            SupportedLanguages::C => C_EXTENSIONS,
            SupportedLanguages::Cpp => CPP_EXTENSIONS,
        }
    }
}

impl From<SupportedLanguages> for tree_sitter::Language {
    /// Converts a `SupportedLanguages` enum to a `tree_sitter::Language` object.
    ///
    /// This implementation allows for the conversion of the supported languages to their respective
    /// tree-sitter language objects, enabling accurate parsing and syntax analysis.
    ///
    /// # Parameters
    /// - `val`: The `SupportedLanguages` enum value to be converted.
    ///
    /// # Returns
    /// A `tree_sitter::Language` object corresponding to the provided `SupportedLanguages` enum
    /// value.
    fn from(val: SupportedLanguages) -> Self {
        match val {
            SupportedLanguages::Rust => tree_sitter_rust::LANGUAGE,
            SupportedLanguages::Python => tree_sitter_python::LANGUAGE,
            SupportedLanguages::Typescript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            SupportedLanguages::Javascript => tree_sitter_javascript::LANGUAGE,
            SupportedLanguages::Ruby => tree_sitter_ruby::LANGUAGE,
            SupportedLanguages::Java => tree_sitter_java::LANGUAGE,
            SupportedLanguages::Go => tree_sitter_go::LANGUAGE,
            SupportedLanguages::Solidity => tree_sitter_solidity::LANGUAGE,
            SupportedLanguages::C => tree_sitter_c::LANGUAGE,
            SupportedLanguages::Cpp => tree_sitter_cpp::LANGUAGE,
        }
        .into()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    pub use strum::IntoEnumIterator as _;

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
        assert_eq!(
            SupportedLanguages::from_str("java"),
            Ok(SupportedLanguages::Java)
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

        assert_eq!(
            SupportedLanguages::from_str("Java"),
            Ok(SupportedLanguages::Java)
        );
        assert_eq!(
            SupportedLanguages::from_str("C++"),
            Ok(SupportedLanguages::Cpp)
        );
        assert_eq!(
            SupportedLanguages::from_str("cpp"),
            Ok(SupportedLanguages::Cpp)
        );
    }

    #[test]
    fn test_serialize_and_deserialize_for_supported_languages() {
        for lang in SupportedLanguages::iter() {
            let val = serde_json::to_string(&lang).unwrap();

            assert_eq!(
                serde_json::to_string(&lang).unwrap(),
                format!("\"{lang}\""),
                "Failed to serialize {lang}"
            );
            assert_eq!(
                serde_json::from_str::<SupportedLanguages>(&val).unwrap(),
                lang,
                "Failed to deserialize {lang}"
            );
            assert_eq!(
                serde_json::from_str::<SupportedLanguages>(&val.to_lowercase()).unwrap(),
                lang,
                "Failed to deserialize lowercase {lang}"
            );
        }
    }
}
