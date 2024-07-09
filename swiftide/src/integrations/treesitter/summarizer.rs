use anyhow::{Context as _, Result};
use tree_sitter::{Node, Parser, TreeCursor};

use derive_builder::Builder;

use super::supported_languages::SupportedLanguages;

#[derive(Debug, Builder, Clone)]
/// Generates a summary of a code file.
///
/// Supports splitting code files into chunks based on a maximum size or a range of bytes.
#[builder(setter(into), build_fn(error = "anyhow::Error"))]
pub struct CodeSummarizer {
    #[builder(setter(custom))]
    language: SupportedLanguages,
}

impl CodeSummarizerBuilder {
    /// Attempts to set the language for the `CodeSummarizer`.
    ///
    /// # Arguments
    ///
    /// * `language` - A value that can be converted into `SupportedLanguages`.
    ///
    /// # Returns
    ///
    /// * `Result<Self>` - The builder instance with the language set, or an error if the language is not supported.
    pub fn try_language(mut self, language: impl TryInto<SupportedLanguages>) -> Result<Self> {
        self.language = Some(
            // For some reason there's a trait conflict, wth
            language
                .try_into()
                .ok()
                .context("Treesitter language not supported")?,
        );
        Ok(self)
    }
}

impl CodeSummarizer {
    /// Creates a new `CodeSummarizer` with the specified language
    ///
    /// # Arguments
    ///
    /// * `language` - The programming language for which the code will be summarized.
    ///
    /// # Returns
    ///
    /// * `Self` - A new instance of `CodeSummarizer`.
    pub fn new(language: SupportedLanguages) -> Self {
        Self { language }
    }

    /// Creates a new builder for `CodeSummarizer`.
    ///
    /// # Returns
    ///
    /// * `CodeSummarizerBuilder` - A new builder instance for `CodeSummarizer`.
    pub fn builder() -> CodeSummarizerBuilder {
        CodeSummarizerBuilder::default()
    }

    /// Summarizes a code file.
    ///
    /// # Arguments
    ///
    /// * `code` - The source code to be split.
    ///
    /// # Returns
    ///
    /// * `Result<String>` - A result containing a string, or an error if the code could not be parsed.
    pub fn summarize(&self, code: &str) -> Result<String> {
        let mut parser = Parser::new();
        parser.set_language(&self.language.into())?;
        let tree = parser.parse(code, None).context("No nodes found")?;
        let root_node = tree.root_node();

        if root_node.has_error() {
            anyhow::bail!("Root node has invalid syntax");
        } else {
            let mut cursor = root_node.walk();
            let mut summary = String::with_capacity(code.len());
            let last_end = 0;
            self.summarize_node(&mut cursor, code, &mut summary, last_end);
            Ok(summary)
        }
    }

    fn is_unneeded_node(&self, node: Node) -> bool {
        // We can use self.language to determine if a node is needed
        match node.kind() {
            "line_comment" => true,
            "comment" => true,
            _ => {
                println!("Node kind: {}", node.kind());
                false
            }
        }
    }

    /// Summarices a syntax node
    ///
    /// # Arguments
    ///
    /// * `node` - The syntax node to be chunked.
    /// * `source` - The source code as a string.
    /// * `last_end` - The end byte of the last chunk.
    ///
    /// # Returns
    ///
    /// * `String` - A summary of the syntax node.
    fn summarize_node(
        &self,
        cursor: &mut TreeCursor,
        source: &str,
        summary: &mut String,
        mut last_end: usize,
    ) {
        let node = cursor.node();
        // If the node is not needed in the summary, skip it and go to the next sibling
        if self.is_unneeded_node(node) {
            last_end = node.end_byte();
            if cursor.goto_next_sibling() {
                self.summarize_node(cursor, source, summary, last_end)
            }
            return;
        }

        println!("not skipped {}", node.kind());

        // If the node is a non-leaf, recursively summarize its children
        if cursor.goto_first_child() {
            self.summarize_node(cursor, source, summary, last_end)
        // If the node is a leaf, add the text to the summary
        } else {
            summary.push_str(&source[last_end..node.end_byte()]);
            last_end = node.end_byte();
        }
        if cursor.goto_next_sibling() {
            self.summarize_node(cursor, source, summary, last_end)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summarize() {
        let code = r#"// This is a comment
fn main(a: usize, b: usize) -> usize {
    println!("Hello, world!");
}"#;
        let summarizer = CodeSummarizer::new(SupportedLanguages::Rust);
        let summary = summarizer.summarize(code).unwrap();
        assert_eq!(summary, "fn main() {\n    println!(\"Hello, world!\");\n}");
    }
}
