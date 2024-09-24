//! Adds references and definitions found in code as metadata to chunks
//!
//! Uses tree-sitter to do the extractions. It tries to only get unique definitions and references,
//! and only references that are not local.
//!
//! See the [`crate::treesitter::CodeParser`] tests for some examples.
//!
//! # Example
//!
//! ```no_run
//! # use swiftide_core::indexing::Node;
//! # use swiftide_integrations::treesitter::transformers::metadata_refs_defs_code::*;
//! # use swiftide_core::Transformer;
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let transformer = MetadataRefsDefsCode::try_from_language("rust").unwrap();
//! let code = r#"
//!   fn main() {
//!     println!("Hello, World!");
//!   }
//! "#;
//! let mut node = Node::new(code.to_string());
//!
//! node = transformer.transform_node(node).await.unwrap();
//!
//! assert_eq!(
//!     node.metadata.get(NAME_REFERENCES).unwrap().as_str().unwrap(),
//!     "println"
//! );
//! assert_eq!(
//!     node.metadata.get(NAME_DEFINITIONS).unwrap().as_str().unwrap(),
//!     "main"
//! );
//! # Ok(())
//! # }
//! ```
use std::sync::Arc;

use swiftide_core::{indexing::Node, Transformer};

use crate::treesitter::{CodeParser, SupportedLanguages};
use anyhow::{Context as _, Result};
use async_trait::async_trait;

pub const NAME_REFERENCES: &str = "References (code)";
pub const NAME_DEFINITIONS: &str = "Definitions (code)";

/// `MetadataRefsDefsCode` is responsible for extracting references and definitions.
#[swiftide_macros::indexing_transformer(derive(skip_default))]
pub struct MetadataRefsDefsCode {
    code_parser: Arc<CodeParser>,
}

impl MetadataRefsDefsCode {
    /// Tries to build a new `MetadataRefsDefsCode` transformer
    ///
    /// # Errors
    ///
    /// Language is not supported by tree-sitter
    pub fn try_from_language(language: impl TryInto<SupportedLanguages>) -> Result<Self> {
        let language: SupportedLanguages = language
            .try_into()
            .ok()
            .context("Treesitter language not supported")?;

        MetadataRefsDefsCode::builder()
            .code_parser(CodeParser::from_language(language))
            .build()
    }
}

#[async_trait]
impl Transformer for MetadataRefsDefsCode {
    /// Extracts references and definitions from code and
    /// adds them as metadata to the node if present
    async fn transform_node(&self, mut node: Node) -> Result<Node> {
        let refs_defs = self
            .code_parser
            .parse(&node.chunk)?
            .references_and_definitions()?;

        if !refs_defs.references.is_empty() {
            node.metadata
                .insert(NAME_REFERENCES.to_string(), refs_defs.references.join(","));
        }

        if !refs_defs.definitions.is_empty() {
            node.metadata.insert(
                NAME_DEFINITIONS.to_string(),
                refs_defs.definitions.join(","),
            );
        }
        Ok(node)
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use test_case::test_case;

    #[test_case("rust", "fn main() { println!(\"Hello, World!\"); }", "println", "main"; "rust")]
    #[test_case("ruby", "def main; puts 'Hello, World!'; end", "puts", "main"; "ruby")]
    #[test_case("python", "def main(): print('Hello, World!')", "print", "main"; "python")]
    #[test_case("javascript", "function main() { console.log('Hello, World!'); }", "log", "main"; "javascript")]
    #[test_case("typescript", "function main() { console.log('Hello, World!'); }", "log", "main"; "typescript")]
    #[test_case("java", "public class Main { public static void main(String[] args) { System.out.println(\"Hello, World!\"); } }", "println", "Main,main"; "java")]
    #[tokio::test]
    async fn assert_refs_defs_from_code(
        lang: &str,
        code: &str,
        expected_references: &str,
        expected_definitions: &str,
    ) {
        let transformer = MetadataRefsDefsCode::try_from_language(lang).unwrap();
        let node = Node::new(code);

        let node = transformer.transform_node(node).await.unwrap();

        let references = node
            .metadata
            .get(NAME_REFERENCES)
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        let definitions = node
            .metadata
            .get(NAME_DEFINITIONS)
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        assert_eq!(references, expected_references);
        assert_eq!(definitions, expected_definitions);
    }
}
