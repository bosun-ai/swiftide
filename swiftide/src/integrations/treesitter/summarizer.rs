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
            let mut last_end = 0;
            self.summarize_node(&mut cursor, code, &mut summary, &mut last_end);
            Ok(summary)
        }
    }

    fn is_unneeded_node(&self, node: Node) -> bool {
        // We can use self.language to determine if a node is needed
        match self.language {
            SupportedLanguages::Rust => match node.kind() {
                "line_comment" => false,
                "block" => true,
                _ => false,
            },
            SupportedLanguages::Typescript => match node.kind() {
                "line_comment" => false,
                "statement_block" => true,
                _ => false,
            },
            SupportedLanguages::Python => match node.kind() {
                "line_comment" => false,
                "block" => {
                    // Check if the node is a function signature
                    let parent = node.parent().unwrap();
                    println!("Parent kind: {}", parent.kind());
                    parent.kind() == "function_definition"
                }
                _ => false,
            },
            SupportedLanguages::Ruby => match node.kind() {
                "line_comment" => false,
                "body_statement" => {
                    // Check if the node is a function signature
                    let parent = node.parent().unwrap();
                    println!("Parent kind: {}", parent.kind());
                    parent.kind() == "def"
                }
                _ => {
                    println!("Node kind: {}", node.kind());
                    // Default to false
                    false
                }
            },
            SupportedLanguages::Javascript => match node.kind() {
                "line_comment" => false,
                "statement_block" => true,
                _ => false,
            },
        }
    }

    /// Summarizes a syntax node
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
        last_end: &mut usize,
    ) {
        let node = cursor.node();
        // If the node is not needed in the summary, skip it and go to the next sibling
        if self.is_unneeded_node(node) {
            summary.push_str(&source[*last_end..node.start_byte()]);
            *last_end = node.end_byte();
            if cursor.goto_next_sibling() {
                self.summarize_node(cursor, source, summary, last_end)
            }
            return;
        }

        let mut next_cursor = cursor.clone();

        // If the node is a non-leaf, recursively summarize its children
        if next_cursor.goto_first_child() {
            self.summarize_node(&mut next_cursor, source, summary, last_end)
        // If the node is a leaf, add the text to the summary
        } else {
            summary.push_str(&source[*last_end..node.end_byte()]);
            *last_end = node.end_byte();
        }

        if cursor.goto_next_sibling() {
            self.summarize_node(cursor, source, summary, last_end)
        } else {
            // Done with this node
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test every supported language.
    // We should strip away all code blocks and leave only imports, comments, function signatures,
    // class, interface and structure definitions and definitions of constants, variables and other members.
    #[test]
    fn test_summarize_rust() {
        let code = r#"
use anyhow::{Context as _, Result};
// This is a comment
fn main(a: usize, b: usize) -> usize {
    println!("Hello, world!");
}

pub struct Bla {
    a: usize
}

impl Bla {
    fn ok(&mut self) {
        self.a = 1;
    }
}"#;
        let summarizer = CodeSummarizer::new(SupportedLanguages::Rust);
        let summary = summarizer.summarize(code).unwrap();
        assert_eq!(
            summary,
            "\nuse anyhow::{Context as _, Result};\n// This is a comment\nfn main(a: usize, b: usize) -> usize \n\npub struct Bla {\n    a: usize\n}\n\nimpl Bla {\n    fn ok(&mut self) \n}"
        );
    }

    #[test]
    fn test_summarize_typescript() {
        let code = r#"
import { Context as _, Result } from 'anyhow';
// This is a comment
function main(a: number, b: number): number {
    console.log("Hello, world!");
}

export class Bla {
    a: number;
}

export interface Bla {
    ok(): void;
}"#;
        let summarizer = CodeSummarizer::new(SupportedLanguages::Typescript);
        let summary = summarizer.summarize(code).unwrap();
        assert_eq!(
            summary,
            "\nimport { Context as _, Result } from 'anyhow';\n// This is a comment\nfunction main(a: number, b: number): number \n\nexport class Bla {\n    a: number;\n}\n\nexport interface Bla {\n    ok(): void;\n}"
        );
    }

    #[test]
    fn test_summarize_python() {
        let code = r#"
import sys
# This is a comment
def main(a: int, b: int) -> int:
    print("Hello, world!")

class Bla:
    def __init__(self):
        self.a = 1

    def ok(self):
        self.a = 1
"#;
        let summarizer = CodeSummarizer::new(SupportedLanguages::Python);
        let summary = summarizer.summarize(code).unwrap();
        assert_eq!(
            summary,
            "\nimport sys\n# This is a comment\ndef main(a: int, b: int) -> int:\n    \n\nclass Bla:\n    def __init__(self):\n        \n\n    def ok(self):\n        "
        );
    }

    #[test]
    fn test_summarize_ruby() {
        let code = r#"
require 'anyhow'
# This is a comment
def main(a, b)
    puts "Hello, world!"
end

class Bla
    def ok
        @a = 1
    end
end
"#;
        let summarizer = CodeSummarizer::new(SupportedLanguages::Ruby);
        let summary = summarizer.summarize(code).unwrap();
        assert_eq!(
            summary,
            "\nrequire 'anyhow'\n# This is a comment\ndef main(a, b)\n    puts \"Hello, world!\"\nend\n\nclass Bla\n    def ok\n        @a = 1\n    end\nend"
        );
    }

    #[test]
    fn test_summarize_javascript() {
        let code = r#"
import { Context as _, Result } from 'anyhow';
// This is a comment
function main(a, b) {
    console.log("Hello, world!");
}

class Bla {
    constructor() {
        this.a = 1;
    }

    ok() {
        this.a = 1;
    }
}
"#;
        let summarizer = CodeSummarizer::new(SupportedLanguages::Javascript);
        let summary = summarizer.summarize(code).unwrap();
        assert_eq!(
            summary,
            "\nimport { Context as _, Result } from 'anyhow';\n// This is a comment\nfunction main(a, b) \n\nclass Bla {\n    constructor() \n\n    ok() \n}"
        );
    }
}
