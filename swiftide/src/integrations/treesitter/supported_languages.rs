#[allow(unused_imports)]
pub use std::str::FromStr as _;

#[derive(Debug, PartialEq, Clone, Copy, strum_macros::EnumString, strum_macros::Display)]
#[strum(ascii_case_insensitive)]
pub enum SupportedLanguages {
    Rust,
    Typescript,
    Python,
    Ruby,
    Javascript,
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

impl From<SupportedLanguages> for tree_sitter::Language {
    fn from(val: SupportedLanguages) -> Self {
        match val {
            SupportedLanguages::Rust => tree_sitter_rust::language(),
            SupportedLanguages::Python => tree_sitter_python::language(),
            SupportedLanguages::Typescript => tree_sitter_typescript::language_typescript(),
            SupportedLanguages::Javascript => tree_sitter_javascript::language(),
            SupportedLanguages::Ruby => tree_sitter_ruby::language(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

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
