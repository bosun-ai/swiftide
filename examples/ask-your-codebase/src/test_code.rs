//! Leaves tests out of the index.
//!
//! Tests mostly restate how the code is used. Leaving them out keeps the retrieved context
//! focused on the implementation. Test detection follows Rust conventions: `tests/` directories,
//! `test_utils` modules and items marked with a test attribute.

use std::{ffi::OsStr, ops::Range, path::Path};

use anyhow::{Context as _, Result};
use swiftide::{indexing::TextNode, integrations::treesitter::SupportedLanguages};
use tree_sitter::{Node, Parser};

use crate::codebase::Codebase;

// ANCHOR: skip_tests
/// Drops test files before they are cached or chunked: files in a `tests/` directory, test
/// modules in their own file (`src/agent/tests.rs`) and test support modules such as
/// `src/test_utils.rs`.
///
/// Errors are kept, so they still surface.
pub fn not_test_files(codebase: Codebase) -> impl Fn(&Result<TextNode>) -> bool + Send + Sync {
    move |file| {
        !file
            .as_ref()
            .is_ok_and(|file| is_test_file(codebase.relative_path(&file.path)))
    }
}

/// Removes unit tests (`#[cfg(test)] mod tests { .. }`, `#[test] fn ..`) from Rust files.
///
/// This runs on whole files, before chunking, so a chunk never mixes implementation and tests.
pub fn without_unit_tests(mut file: TextNode) -> Result<TextNode> {
    if file
        .path
        .extension()
        .is_some_and(|extension| extension == "rs")
    {
        file.chunk = strip_test_items(&file.chunk)?;
    }
    Ok(file)
}
// ANCHOR_END: skip_tests

/// File names (without extension) of modules that only exist to support tests.
///
/// `tests` covers the body of `#[cfg(test)] mod tests;` in `src/<module>/tests.rs`.
const TEST_SUPPORT_MODULES: &[&str] = &["tests", "test_utils", "test_helpers"];

fn is_test_file(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == "tests")
        || path
            .file_stem()
            .and_then(OsStr::to_str)
            .is_some_and(|stem| TEST_SUPPORT_MODULES.contains(&stem))
}

fn strip_test_items(source: &str) -> Result<String> {
    let mut parser = Parser::new();
    parser.set_language(&SupportedLanguages::Rust.into())?;
    let tree = parser
        .parse(source, None)
        .context("tree-sitter could not parse the file")?;

    let mut tests = Vec::new();
    collect_test_items(tree.root_node(), source, &mut tests);

    let mut kept = String::with_capacity(source.len());
    let mut position = 0;
    for test in tests {
        kept.push_str(&source[position..test.start]);
        position = test.end;
    }
    kept.push_str(&source[position..]);
    Ok(kept)
}

/// Collects the byte ranges of test items, with their doc comments and attributes, among the
/// items in `container`, in source order. Recurses into inline modules that are not tests
/// themselves.
fn collect_test_items(container: Node<'_>, source: &str, tests: &mut Vec<Range<usize>>) {
    // Start of the comments and attributes directly above the next item
    let mut item_start = None;
    let mut is_test = false;

    let mut cursor = container.walk();
    for item in container.named_children(&mut cursor) {
        match item.kind() {
            "attribute_item" => {
                item_start.get_or_insert(item.start_byte());
                is_test |= is_test_attribute(&source[item.byte_range()]);
            }
            // Inner doc comments (`//!`) document the container, not the next item
            "line_comment" | "block_comment"
                if is_inner_doc_comment(&source[item.byte_range()]) =>
            {
                item_start = None;
                is_test = false;
            }
            "line_comment" | "block_comment" => {
                item_start.get_or_insert(item.start_byte());
            }
            _ => {
                if is_test {
                    tests.push(item_start.unwrap_or(item.start_byte())..item.end_byte());
                } else if let Some(body) = item.child_by_field_name("body")
                    && item.kind() == "mod_item"
                {
                    collect_test_items(body, source, tests);
                }
                item_start = None;
                is_test = false;
            }
        }
    }
}

fn is_inner_doc_comment(comment: &str) -> bool {
    comment.starts_with("//!") || comment.starts_with("/*!")
}

/// `#[cfg(test)]`, `#[test]`, `#[tokio::test(..)]` and the like.
fn is_test_attribute(attribute: &str) -> bool {
    let attribute: String = attribute.split_whitespace().collect();
    let path = attribute
        .trim_start_matches("#[")
        .split(['(', ']'])
        .next()
        .unwrap_or_default();

    attribute == "#[cfg(test)]" || path == "test" || path.ends_with("::test")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_test_modules_and_test_functions() {
        let source = r#"
//! Arithmetic.

/// Adds two numbers.
pub fn add(a: u8, b: u8) -> u8 {
    a + b
}

/// Checks addition.
#[test]
fn adds() {}

/// Shared by the tests below.
#[cfg(test)]
fn test_helper() {}

mod inner {
    pub fn keep() {}

    #[tokio::test(flavor = "multi_thread")]
    async fn drop_me() {}
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}
"#;
        let kept = strip_test_items(source).unwrap();

        assert!(kept.contains("//! Arithmetic."));
        assert!(kept.contains("/// Adds two numbers."));
        assert!(kept.contains("pub fn add"));
        assert!(kept.contains("pub fn keep"));
        assert!(!kept.contains("fn adds"));
        assert!(!kept.contains("Checks addition"));
        assert!(!kept.contains("test_helper"));
        assert!(!kept.contains("Shared by the tests"));
        assert!(!kept.contains("drop_me"));
        assert!(!kept.contains("mod tests"));
        assert!(!kept.contains("#[cfg(test)]"));
    }

    #[test]
    fn recognizes_test_files() {
        assert!(is_test_file(Path::new("tests/agent.rs")));
        assert!(is_test_file(Path::new("src/tests/mod.rs")));
        assert!(is_test_file(Path::new("src/test_utils.rs")));
        assert!(is_test_file(Path::new("src/agent/tests.rs")));
        assert!(!is_test_file(Path::new("src/agent.rs")));
        assert!(!is_test_file(Path::new("src/testing_tools.rs")));
    }

    #[test]
    fn keeps_attributes_that_merely_mention_test() {
        assert!(!is_test_attribute("#[cfg(any(test, debug_assertions))]"));
        assert!(!is_test_attribute("#[derive(Debug)]"));
        assert!(is_test_attribute("#[cfg( test )]"));
        assert!(is_test_attribute("#[test_log::test(tokio::test)]"));
    }
}
