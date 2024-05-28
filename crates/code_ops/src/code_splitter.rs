#![allow(dead_code)]
extern crate tree_sitter;

use anyhow::{Context as _, Result};
use models::SupportedLanguages;
use std::ops::Range;
use tree_sitter::{Node, Parser};

use derive_builder::Builder;

use crate::supported_language_to_tree_sitter;

// TODO: Instead of counting bytes, count tokens with titktoken
const DEFAULT_MAX_BYTES: usize = 1500;

#[derive(Debug, Builder)]
/// Splits code files into meaningful chunks
///
/// Supports splitting code files into chunks based on a maximum size or a range of bytes.
pub struct CodeSplitter {
    /// Maximum size of a chunk in bytes or a range of bytes
    #[builder(default, setter(into))]
    chunk_size: ChunkSize,
    language: SupportedLanguages,
}

#[derive(Debug, Clone)]
pub enum ChunkSize {
    Bytes(usize),
    Range(Range<usize>),
}

impl From<usize> for ChunkSize {
    fn from(size: usize) -> Self {
        ChunkSize::Bytes(size)
    }
}

impl From<Range<usize>> for ChunkSize {
    fn from(range: Range<usize>) -> Self {
        ChunkSize::Range(range)
    }
}

impl Default for ChunkSize {
    fn default() -> Self {
        ChunkSize::Bytes(DEFAULT_MAX_BYTES)
    }
}

impl CodeSplitter {
    pub fn new(language: SupportedLanguages) -> Self {
        Self {
            chunk_size: Default::default(),
            language,
        }
    }

    pub fn builder() -> CodeSplitterBuilder {
        CodeSplitterBuilder::default()
    }

    fn chunk_node(&self, node: Node, source: &str, mut last_end: usize) -> Vec<String> {
        let mut new_chunks: Vec<String> = Vec::new();
        let mut current_chunk = String::new();

        for child in node.children(&mut node.walk()) {
            if child.end_byte() - child.start_byte() > self.max_bytes() {
                // Child is too big, recursively chunk the child
                if !current_chunk.is_empty() && current_chunk.len() > self.min_bytes() {
                    new_chunks.push(current_chunk);
                }
                current_chunk = String::new();
                new_chunks.extend(self.chunk_node(child, source, last_end));
            } else if current_chunk.len() + child.end_byte() - child.start_byte() > self.max_bytes()
            {
                // Child would make the current chunk too big, so start a new chunk
                new_chunks.push(current_chunk.trim().to_string());
                current_chunk = source[last_end..child.end_byte()].to_string();
            } else {
                current_chunk += &source[last_end..child.end_byte()];
            }
            last_end = child.end_byte();
        }

        if !current_chunk.is_empty() && current_chunk.len() > self.min_bytes() {
            new_chunks.push(current_chunk)
        }

        new_chunks
    }

    pub fn split(&self, code: &str) -> Result<Vec<String>> {
        let mut parser = Parser::new();
        parser.set_language(&supported_language_to_tree_sitter(&self.language))?;
        let tree = parser.parse(code, None).context("No nodes found")?;
        let root_node = tree.root_node();

        if root_node.has_error() {
            anyhow::bail!("Root node has invalid syntax");
        } else {
            Ok(self.chunk_node(root_node, code, 0))
        }
    }

    fn max_bytes(&self) -> usize {
        match &self.chunk_size {
            ChunkSize::Bytes(size) => *size,
            ChunkSize::Range(range) => range.end,
        }
    }

    fn min_bytes(&self) -> usize {
        if let ChunkSize::Range(range) = &self.chunk_size {
            range.start
        } else {
            0
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

        let splitter = CodeSplitter::new(SupportedLanguages::Rust);

        let chunks = splitter.split(code);

        assert_eq!(chunks.unwrap(), vec!["fn hello_world() {}"]);
    }

    #[test]
    fn test_chunk_lines() {
        let splitter = CodeSplitter::new(SupportedLanguages::Rust);

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
        let splitter = CodeSplitter::builder()
            .language(SupportedLanguages::Rust)
            .chunk_size(50)
            .build()
            .unwrap();

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
                "\n    println!(\"Goodbye, World!\");\n}",
            ]
        )
    }

    #[test]
    fn test_empty_text() {
        let splitter = CodeSplitter::builder()
            .language(SupportedLanguages::Rust)
            .chunk_size(50)
            .build()
            .unwrap();

        let text = "";
        let chunks = splitter.split(text).unwrap();

        dbg!(&chunks);
        assert_eq!(chunks.len(), 0);
    }

    #[test]
    fn test_range_max() {
        let splitter = CodeSplitter::builder()
            .language(SupportedLanguages::Rust)
            .chunk_size(0..50)
            .build()
            .unwrap();

        let text = indoc! {r#"
            fn main() {
                println!("Hello, World!");
                println!("Goodbye, World!");
            }
        "#};
        let chunks = splitter.split(text).unwrap();
        assert_eq!(
            chunks,
            vec![
                "fn main()",
                "{\n    println!(\"Hello, World!\");",
                "\n    println!(\"Goodbye, World!\");\n}",
            ]
        )
    }

    #[test]
    fn test_range_min_and_max() {
        let splitter = CodeSplitter::builder()
            .language(SupportedLanguages::Rust)
            .chunk_size(20..50)
            .build()
            .unwrap();
        let text = indoc! {r#"
            fn main() {
                println!("Hello, World!");
                println!("Goodbye, World!");
            }
        "#};
        let chunks = splitter.split(text).unwrap();
        assert_eq!(
            chunks,
            vec![
                "{\n    println!(\"Hello, World!\");",
                "\n    println!(\"Goodbye, World!\");\n}",
            ]
        )
    }
}
