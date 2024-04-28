#![allow(dead_code)]
extern crate tree_sitter;

use anyhow::{Context as _, Result};
use infrastructure::supported_languages::SupportedLanguages;
use tree_sitter::{Language, Node, Parser};

const DEFAULT_MAX_BYTES: usize = 1500;

pub struct CodeSplitter {
    max_bytes: usize,
    parser: Parser,
}

fn try_map_language(language: &SupportedLanguages) -> Result<Language> {
    match language {
        SupportedLanguages::Rust => Ok(tree_sitter_rust::language()),
        _ => anyhow::bail!("Language {language} not supported by code splitter"),
    }
}

/// Splits code files into meaningful chunks
impl CodeSplitter {
    pub fn try_new(language: SupportedLanguages, max_bytes: Option<usize>) -> Result<Self> {
        let mut parser = Parser::new();
        parser.set_language(&try_map_language(&language)?)?;

        Ok(Self {
            parser,
            max_bytes: max_bytes.unwrap_or(DEFAULT_MAX_BYTES),
        })
    }

    pub fn try_from_defaults(language: SupportedLanguages) -> Result<Self> {
        Self::try_new(language, None)
    }

    fn chunk_node(&self, node: Node, source: &str, mut last_end: usize) -> Vec<String> {
        let mut new_chunks: Vec<String> = Vec::new();
        let mut current_chunk = String::new();

        for child in node.children(&mut node.walk()) {
            if child.end_byte() - child.start_byte() > self.max_bytes {
                // Child is too big, recursively chunk the child
                if !current_chunk.is_empty() {
                    new_chunks.push(current_chunk);
                }
                current_chunk = String::new();
                new_chunks.extend(self.chunk_node(child, source, last_end));
            } else if current_chunk.len() + child.end_byte() - child.start_byte() > self.max_bytes {
                // Child would make the current chunk too big, so start a new chunk
                new_chunks.push(current_chunk.trim().to_string());
                current_chunk = source[last_end..child.end_byte()].trim().to_string();
            } else {
                current_chunk += &source[last_end..child.end_byte()];
            }
            last_end = child.end_byte();
        }

        if !current_chunk.is_empty() {
            new_chunks.push(current_chunk)
        }

        new_chunks
    }

    pub fn split(&mut self, code: &str) -> Result<Vec<String>> {
        let tree = self.parser.parse(code, None).context("No nodes found")?;
        let root_node = tree.root_node();

        if root_node.has_error() {
            anyhow::bail!("Root node has invalid syntax");
        } else {
            Ok(self.chunk_node(root_node, code, 0))
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use indoc::indoc;

    #[test]
    fn test_split_single_chunk() {
        let code = "fn hello_world() {}";

        let mut splitter = CodeSplitter::try_from_defaults(SupportedLanguages::Rust).unwrap();

        let chunks = splitter.split(code);

        assert_eq!(chunks.unwrap(), vec!["fn hello_world() {}"]);
    }

    #[test]
    fn test_chunk_lines() {
        let mut splitter = CodeSplitter::try_new(SupportedLanguages::Rust, None)
            .expect("Failed to create CodeSplitter.");

        let text = indoc! {r#"
            fn main() {
                println!("Hello");
                println!("World");
                println!("!");
            }
        "#};

        let chunks = splitter.split(text).unwrap();

        dbg!(&chunks);
        assert_eq!(chunks.len(), 1);
        assert_eq!(
            chunks[0],
            "fn main() {\n    println!(\"Hello\");\n    println!(\"World\");\n    println!(\"!\");\n}"
        );
    }

    #[test]
    fn test_max_bytes_limit() {
        let mut splitter = CodeSplitter::try_new(
            SupportedLanguages::Rust,
            Some(50), // Max 50 bytes
        )
        .expect("Failed to create CodeSplitter.");

        let text = indoc! {r#"
            fn main() {
                println!("Hello, World!");
                println!("Goodbye, World!");
            }
        "#};
        let chunks = splitter.split(text).unwrap();

        dbg!(&chunks);
        assert_eq!(
            chunks,
            vec![
                "fn main()",
                "{\n    println!(\"Hello, World!\");",
                "println!(\"Goodbye, World!\");\n}",
            ]
        )
    }

    #[test]
    fn test_empty_text() {
        let mut splitter = CodeSplitter::try_new(
            SupportedLanguages::Rust,
            Some(50), // Max 50 characters
        )
        .expect("Failed to create CodeSplitter.");

        let text = "";
        let chunks = splitter.split(text).unwrap();

        dbg!(&chunks);
        assert_eq!(chunks.len(), 0);
    }
}
