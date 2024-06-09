mod code_parser;
mod code_splitter;
mod supported_languages;

pub use supported_languages::SupportedLanguages;
use tree_sitter::Language;
pub use {code_parser::CodeParser, code_splitter::ChunkSize, code_splitter::CodeSplitter};

pub(crate) fn supported_language_to_tree_sitter(language: &SupportedLanguages) -> Language {
    match language {
        SupportedLanguages::Rust => tree_sitter_rust::language(),
        SupportedLanguages::Python => tree_sitter_python::language(),
        SupportedLanguages::Typescript => tree_sitter_typescript::language_typescript(),
        SupportedLanguages::Javascript => tree_sitter_javascript::language(),
        SupportedLanguages::Ruby => tree_sitter_ruby::language(),
    }
}
