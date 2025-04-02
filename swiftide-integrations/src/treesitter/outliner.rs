use anyhow::{Context as _, Result};
use tree_sitter::{Node, Parser, TreeCursor};

use derive_builder::Builder;

use super::supported_languages::SupportedLanguages;

#[derive(Debug, Builder, Clone)]
/// Generates a summary of a code file.
///
/// It does so by parsing the code file and removing function bodies, leaving only the function
/// signatures and other top-level declarations along with any comments.
///
/// The resulting summary can be used as a context when considering subsets of the code file, or for
/// determining relevance of the code file to a given task.
#[builder(setter(into), build_fn(error = "anyhow::Error"))]
pub struct CodeOutliner {
    #[builder(setter(custom))]
    language: SupportedLanguages,
}

impl CodeOutlinerBuilder {
    /// Attempts to set the language for the `CodeOutliner`.
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
    /// * If the language is not supported, an error is returned.
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

impl CodeOutliner {
    /// Creates a new `CodeOutliner` with the specified language
    ///
    /// # Arguments
    ///
    /// * `language` - The programming language for which the code will be outlined.
    ///
    /// # Returns
    ///
    /// * `Self` - A new instance of `CodeOutliner`.
    pub fn new(language: SupportedLanguages) -> Self {
        Self { language }
    }

    /// Creates a new builder for `CodeOutliner`.
    ///
    /// # Returns
    ///
    /// * `CodeOutlinerBuilder` - A new builder instance for `CodeOutliner`.
    pub fn builder() -> CodeOutlinerBuilder {
        CodeOutlinerBuilder::default()
    }

    /// outlines a code file.
    ///
    /// # Arguments
    ///
    /// * `code` - The source code to be split.
    ///
    /// # Returns
    ///
    /// * `Result<String>` - A result containing a string, or an error if the code could not be
    ///   parsed.
    ///
    /// # Errors
    /// * If the code could not be parsed, an error is returned.
    pub fn outline(&self, code: &str) -> Result<String> {
        let mut parser = Parser::new();
        parser.set_language(&self.language.into())?;
        let tree = parser.parse(code, None).context("No nodes found")?;
        let root_node = tree.root_node();

        if root_node.has_error() {
            anyhow::bail!("Root node has invalid syntax");
        }

        let mut cursor = root_node.walk();
        let mut summary = String::with_capacity(code.len());
        let mut last_end = 0;
        self.outline_node(&mut cursor, code, &mut summary, &mut last_end);
        Ok(summary)
    }

    fn is_unneeded_node(&self, node: Node) -> bool {
        match self.language {
            SupportedLanguages::Rust | SupportedLanguages::Java => matches!(node.kind(), "block"),
            SupportedLanguages::Typescript | SupportedLanguages::Javascript => {
                matches!(node.kind(), "statement_block")
            }
            SupportedLanguages::Python => match node.kind() {
                "block" => {
                    let parent = node.parent().expect("Python block node has no parent");
                    parent.kind() == "function_definition"
                }
                _ => false,
            },
            SupportedLanguages::Ruby => match node.kind() {
                "body_statement" => {
                    let parent = node
                        .parent()
                        .expect("Ruby body_statement node has no parent");
                    parent.kind() == "method"
                }
                _ => false,
            },
            SupportedLanguages::Go => unimplemented!(),
            SupportedLanguages::Solidity => unimplemented!(),
            SupportedLanguages::C => unimplemented!(),
            SupportedLanguages::Cpp => unimplemented!(),
        }
    }

    /// outlines a syntax node
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
    fn outline_node(
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
                self.outline_node(cursor, source, summary, last_end);
            }
            return;
        }

        let mut next_cursor = cursor.clone();

        // If the node is a non-leaf, recursively outline its children
        if next_cursor.goto_first_child() {
            self.outline_node(&mut next_cursor, source, summary, last_end);
        // If the node is a leaf, add the text to the summary
        } else {
            summary.push_str(&source[*last_end..node.end_byte()]);
            *last_end = node.end_byte();
        }

        if cursor.goto_next_sibling() {
            self.outline_node(cursor, source, summary, last_end);
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
    // class, interface and structure definitions and definitions of constants, variables and other
    // members.
    #[test]
    fn test_outline_rust() {
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
        let outliner = CodeOutliner::new(SupportedLanguages::Rust);
        let summary = outliner.outline(code).unwrap();
        assert_eq!(
            summary,
            "\nuse anyhow::{Context as _, Result};\n// This is a comment\nfn main(a: usize, b: usize) -> usize \n\npub struct Bla {\n    a: usize\n}\n\nimpl Bla {\n    fn ok(&mut self) \n}"
        );
    }

    #[test]
    fn test_outline_typescript() {
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
        let outliner = CodeOutliner::new(SupportedLanguages::Typescript);
        let summary = outliner.outline(code).unwrap();
        assert_eq!(
            summary,
            "\nimport { Context as _, Result } from 'anyhow';\n// This is a comment\nfunction main(a: number, b: number): number \n\nexport class Bla {\n    a: number;\n}\n\nexport interface Bla {\n    ok(): void;\n}"
        );
    }

    #[test]
    fn test_outline_python() {
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
        let outliner = CodeOutliner::new(SupportedLanguages::Python);
        let summary = outliner.outline(code).unwrap();
        assert_eq!(
            summary,
            "\nimport sys\n# This is a comment\ndef main(a: int, b: int) -> int:\n    \n\nclass Bla:\n    def __init__(self):\n        \n\n    def ok(self):\n        "
        );
    }

    #[test]
    fn test_outline_ruby() {
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
        let outliner = CodeOutliner::new(SupportedLanguages::Ruby);
        let summary = outliner.outline(code).unwrap();
        assert_eq!(
            summary,
            "\nrequire 'anyhow'\n# This is a comment\ndef main(a, b)\n    \nend\n\nclass Bla\n    def ok\n        \n    end\nend"
        );
    }

    #[test]
    fn test_outline_javascript() {
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
        let outliner = CodeOutliner::new(SupportedLanguages::Javascript);
        let summary = outliner.outline(code).unwrap();
        assert_eq!(
            summary,
            "\nimport { Context as _, Result } from 'anyhow';\n// This is a comment\nfunction main(a, b) \n\nclass Bla {\n    constructor() \n\n    ok() \n}"
        );
    }

    #[test]
    fn test_outline_java() {
        let code = r#"
import java.io.PrintStream;
import java.util.Scanner;

public class HelloWorld {
    // This is a comment
    public static void main(String[] args) {
        PrintStream out = System.out;

        out.println("Hello, World!");
    }
}
"#;
        let outliner = CodeOutliner::new(SupportedLanguages::Java);
        let summary = outliner.outline(code).unwrap();
        println!("{summary}");
        assert_eq!(
            summary,
            "\nimport java.io.PrintStream;\nimport java.util.Scanner;\n\npublic class HelloWorld {\n    // This is a comment\n    public static void main(String[] args) \n}"
        );
    }
}
