use serde::{Deserialize, Serialize};
pub use std::str::FromStr;
use strum::EnumString;

#[derive(
    Deserialize,
    Serialize,
    Debug,
    PartialEq,
    EnumString,
    Clone,
    Copy,
    strum_macros::EnumIter,
    strum_macros::Display,
)]
#[strum(serialize_all = "snake_case", ascii_case_insensitive)]
#[serde(try_from = "String", rename_all = "snake_case")]
pub enum SupportedLanguages {
    Rust,
    Typescript,
    Python,
    Ruby,
    Javascript,
}

// NOTE: These froms are weird, should be into? Also, should be some way to let either serde or
// strum handle this
impl From<SupportedLanguages> for String {
    fn from(val: SupportedLanguages) -> Self {
        match val {
            SupportedLanguages::Rust => "rust".to_owned(),
            SupportedLanguages::Typescript => "typescript".to_owned(),
            SupportedLanguages::Javascript => "javascript".to_owned(),
            SupportedLanguages::Python => "python".to_owned(),
            SupportedLanguages::Ruby => "ruby".to_owned(),
        }
    }
}

impl From<SupportedLanguages> for &str {
    fn from(val: SupportedLanguages) -> Self {
        match val {
            SupportedLanguages::Rust => "rust",
            SupportedLanguages::Typescript => "typescript",
            SupportedLanguages::Javascript => "javascript",
            SupportedLanguages::Python => "python",
            SupportedLanguages::Ruby => "ruby",
        }
    }
}

impl TryFrom<String> for SupportedLanguages {
    type Error = strum::ParseError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        SupportedLanguages::from_str(&value)
    }
}

static RUST_EXTENSIONS: &[&str] = &["rs"];
static TYPESCRIPT_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx"];
static PYTHON_EXTENSIONS: &[&str] = &["py"];
static RUBY_EXTENSIONS: &[&str] = &["rb"];
static JAVASCRIPT_EXTENSIONS: &[&str] = &["js", "jsx"];

impl SupportedLanguages {
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

#[cfg(test)]
mod test {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_supported_languages_into_string() {
        assert_eq!(SupportedLanguages::Rust.to_string(), "rust");
        assert_eq!(SupportedLanguages::Typescript.to_string(), "typescript");
    }

    #[test]
    fn test_supported_languages_into_str() {
        assert_eq!(Into::<&str>::into(SupportedLanguages::Rust), "rust");
        assert_eq!(
            Into::<&str>::into(SupportedLanguages::Typescript),
            "typescript"
        );
    }

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
