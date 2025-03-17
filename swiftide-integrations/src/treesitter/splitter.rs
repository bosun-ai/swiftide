use anyhow::{Context as _, Result};
use std::ops::Range;
use tree_sitter::{Node, Parser};

use derive_builder::Builder;

use super::supported_languages::SupportedLanguages;

// TODO: Instead of counting bytes, count tokens with titktoken
const DEFAULT_MAX_BYTES: usize = 1500;

#[derive(Debug, Builder, Clone)]
/// Splits code files into meaningful chunks
///
/// Supports splitting code files into chunks based on a maximum size or a range of bytes.
#[builder(setter(into), build_fn(error = "anyhow::Error"))]
pub struct CodeSplitter {
    /// Maximum size of a chunk in bytes or a range of bytes
    #[builder(default, setter(into))]
    chunk_size: ChunkSize,
    #[builder(setter(custom))]
    language: SupportedLanguages,
}

impl CodeSplitterBuilder {
    /// Attempts to set the language for the `CodeSplitter`.
    ///
    /// # Arguments
    ///
    /// * `language` - A value that can be converted into `SupportedLanguages`.
    ///
    /// # Returns
    ///
    /// * `Result<Self>` - The builder instance with the language set, or an error if the language
    ///   is not supported.
    ///
    /// # Errors
    ///
    /// Errors if language is not supported
    pub fn try_language(mut self, language: impl TryInto<SupportedLanguages>) -> Result<Self> {
        self.language = Some(
            language
                .try_into()
                .ok()
                .context("Treesitter language not supported")?,
        );
        Ok(self)
    }
}

#[derive(Debug, Clone)]
/// Represents the size of a chunk, either as a fixed number of bytes or a range of bytes.
pub enum ChunkSize {
    Bytes(usize),
    Range(Range<usize>),
}

impl From<usize> for ChunkSize {
    /// Converts a `usize` into a `ChunkSize::Bytes` variant.
    fn from(size: usize) -> Self {
        ChunkSize::Bytes(size)
    }
}

impl From<Range<usize>> for ChunkSize {
    /// Converts a `Range<usize>` into a `ChunkSize::Range` variant.
    fn from(range: Range<usize>) -> Self {
        ChunkSize::Range(range)
    }
}

impl Default for ChunkSize {
    /// Provides a default value for `ChunkSize`, which is `ChunkSize::Bytes(DEFAULT_MAX_BYTES)`.
    fn default() -> Self {
        ChunkSize::Bytes(DEFAULT_MAX_BYTES)
    }
}

impl CodeSplitter {
    /// Creates a new `CodeSplitter` with the specified language and default chunk size.
    ///
    /// # Arguments
    ///
    /// * `language` - The programming language for which the code will be split.
    ///
    /// # Returns
    ///
    /// * `Self` - A new instance of `CodeSplitter`.
    pub fn new(language: SupportedLanguages) -> Self {
        Self {
            chunk_size: ChunkSize::default(),
            language,
        }
    }

    /// Creates a new builder for `CodeSplitter`.
    ///
    /// # Returns
    ///
    /// * `CodeSplitterBuilder` - A new builder instance for `CodeSplitter`.
    pub fn builder() -> CodeSplitterBuilder {
        CodeSplitterBuilder::default()
    }

    /// Recursively chunks a syntax node into smaller pieces based on the chunk size.
    ///
    /// # Arguments
    ///
    /// * `node` - The syntax node to be chunked.
    /// * `source` - The source code as a string.
    /// * `last_end` - The end byte of the last chunk.
    ///
    /// # Returns
    ///
    /// * `Vec<String>` - A vector of code chunks as strings.
    fn chunk_node(
        &self,
        node: Node,
        source: &str,
        mut last_end: usize,
        current_chunk: Option<String>,
    ) -> Vec<String> {
        let mut new_chunks: Vec<String> = Vec::new();
        let mut current_chunk = current_chunk.unwrap_or_default();

        for child in node.children(&mut node.walk()) {
            debug_assert!(
                current_chunk.len() <= self.max_bytes(),
                "Chunk too big: {} > {}",
                current_chunk.len(),
                self.max_bytes()
            );

            // if the next child will make the chunk too big then there are two options:
            // 1. if the next child is too big to fit in a whole chunk, then recursively chunk it
            //    one level down
            // 2. if the next child is small enough to fit in a chunk, then add the current chunk to
            //    the list and start a new chunk

            let next_child_size = child.end_byte() - last_end;
            if current_chunk.len() + next_child_size >= self.max_bytes() {
                if next_child_size > self.max_bytes() {
                    let mut sub_chunks =
                        self.chunk_node(child, source, last_end, Some(current_chunk));
                    current_chunk = sub_chunks.pop().unwrap_or_default();
                    new_chunks.extend(sub_chunks);
                } else {
                    // NOTE: if the current chunk was smaller than then the min_bytes, then it is
                    // discarded here
                    if !current_chunk.is_empty() && current_chunk.len() > self.min_bytes() {
                        new_chunks.push(current_chunk);
                    }
                    current_chunk = source[last_end..child.end_byte()].to_string();
                }
            } else {
                current_chunk += &source[last_end..child.end_byte()];
            }

            last_end = child.end_byte();
        }

        if !current_chunk.is_empty() && current_chunk.len() > self.min_bytes() {
            new_chunks.push(current_chunk);
        }

        new_chunks
    }

    /// Splits the given code into chunks based on the chunk size.
    ///
    /// # Arguments
    ///
    /// * `code` - The source code to be split.
    ///
    /// # Returns
    ///
    /// * `Result<Vec<String>>` - A result containing a vector of code chunks as strings, or an
    ///   error if the code could not be parsed.
    ///
    /// # Errors
    ///
    /// Returns an error if the node cannot be found or fails to parse
    pub fn split(&self, code: &str) -> Result<Vec<String>> {
        let mut parser = Parser::new();
        parser.set_language(&self.language.into())?;
        let tree = parser.parse(code, None).context("No nodes found")?;
        let root_node = tree.root_node();

        if root_node.has_error() {
            tracing::error!("Syntax error parsing code: {:?}", code);
            return Ok(vec![code.to_string()]);
        }

        Ok(self.chunk_node(root_node, code, 0, None))
    }

    /// Returns the maximum number of bytes allowed in a chunk.
    ///
    /// # Returns
    ///
    /// * `usize` - The maximum number of bytes in a chunk.
    fn max_bytes(&self) -> usize {
        match &self.chunk_size {
            ChunkSize::Bytes(size) => *size,
            ChunkSize::Range(range) => range.end,
        }
    }

    /// Returns the minimum number of bytes allowed in a chunk.
    ///
    /// # Returns
    ///
    /// * `usize` - The minimum number of bytes in a chunk.
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
            .try_language(SupportedLanguages::Rust)
            .unwrap()
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

        assert!(chunks.iter().all(|chunk| chunk.len() <= 50));
        assert!(chunks
            .windows(2)
            .all(|pair| pair.iter().map(String::len).sum::<usize>() >= 50));

        assert_eq!(
            chunks,
            vec![
                "fn main() {\n    println!(\"Hello, World!\");",
                "\n    println!(\"Goodbye, World!\");\n}",
            ]
        );
    }

    #[test]
    fn test_empty_text() {
        let splitter = CodeSplitter::builder()
            .try_language(SupportedLanguages::Rust)
            .unwrap()
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
            .try_language(SupportedLanguages::Rust)
            .unwrap()
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
                "fn main() {\n    println!(\"Hello, World!\");",
                "\n    println!(\"Goodbye, World!\");\n}",
            ]
        );
    }

    #[test]
    fn test_range_min_and_max() {
        let splitter = CodeSplitter::builder()
            .try_language(SupportedLanguages::Rust)
            .unwrap()
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

        assert!(chunks.iter().all(|chunk| chunk.len() <= 50));
        assert!(chunks
            .windows(2)
            .all(|pair| pair.iter().map(String::len).sum::<usize>() > 50));
        assert!(chunks.iter().all(|chunk| chunk.len() >= 20));

        assert_eq!(
            chunks,
            vec![
                "fn main() {\n    println!(\"Hello, World!\");",
                "\n    println!(\"Goodbye, World!\");\n}"
            ]
        );
    }

    #[test]
    fn test_on_self() {
        // read the current file
        let code = include_str!("splitter.rs");
        // try chunking with varying ranges of bytes, give me ten with different min and max
        let ranges = vec![
            10..200,
            50..100,
            100..150,
            150..200,
            200..250,
            250..300,
            300..350,
            350..400,
            400..450,
            450..500,
        ];

        for range in ranges {
            let min = range.start;
            let max = range.end;
            let splitter = CodeSplitter::builder()
                .try_language("rust")
                .unwrap()
                .chunk_size(range)
                .build()
                .unwrap();

            assert_eq!(splitter.min_bytes(), min);
            assert_eq!(splitter.max_bytes(), max);

            let chunks = splitter.split(code).unwrap();

            assert!(chunks.iter().all(|chunk| chunk.len() <= max));
            let chunk_pairs_that_are_smaller_than_max = chunks
                .windows(2)
                .filter(|pair| pair.iter().map(String::len).sum::<usize>() < max);
            assert!(
                chunk_pairs_that_are_smaller_than_max.clone().count() == 0,
                "max: {}, {} + {}, {:?}",
                max,
                chunk_pairs_that_are_smaller_than_max
                    .clone()
                    .next()
                    .unwrap()[0]
                    .len(),
                chunk_pairs_that_are_smaller_than_max
                    .clone()
                    .next()
                    .unwrap()[1]
                    .len(),
                chunk_pairs_that_are_smaller_than_max
                    .collect::<Vec<_>>()
                    .first()
            );
            assert!(chunks.iter().all(|chunk| chunk.len() >= min));

            assert!(
                chunks.iter().all(|chunk| chunk.len() >= min),
                "{:?}",
                chunks
                    .iter()
                    .filter(|chunk| chunk.len() < min)
                    .collect::<Vec<_>>()
            );
            assert!(
                chunks.iter().all(|chunk| chunk.len() <= max),
                "max = {}, chunks = {:?}",
                max,
                chunks
                    .iter()
                    .filter(|chunk| chunk.len() > max)
                    .collect::<Vec<_>>()
            );
        }

        // assert there are no nodes smaller than 10
    }
}
