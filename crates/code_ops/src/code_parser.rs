#![allow(dead_code)]
use tree_sitter::{Language, Node, Parser, Tree};

use anyhow::{Context as _, Result};
use infrastructure::supported_languages::SupportedLanguages;

pub struct CodeParser {
    parser: Parser,
}

fn try_map_language(language: &SupportedLanguages) -> Result<Language> {
    match language {
        SupportedLanguages::Rust => Ok(tree_sitter_rust::language()),
        _ => anyhow::bail!("Language {language} not supported by code splitter"),
    }
}

pub struct CodeNode {
    // parent: Option<Box<CodeNode<'a>>>,
    children: Vec<CodeNode>,
    pub kind: String,
    pub grammar_name: String,
    pub name: String,
}

pub struct CodeTree {
    pub root_node: CodeNode,
    ts_tree: Tree,
}

impl CodeTree {
    // Walks over the tree tracking the depth of the node, allowing to call a function with the
    // depth and the node
    #[allow(clippy::only_used_in_recursion)]
    pub fn walk<T>(&self, node: &CodeNode, depth: usize, f: &impl Fn(usize, &CodeNode) -> T) -> T {
        let res = f(depth, node);
        for child in &node.children {
            self.walk(child, depth + 1, f);
        }
        res
    }
}

impl CodeParser {
    pub fn try_new(language: SupportedLanguages) -> Result<Self> {
        let mut parser = Parser::new();
        parser.set_language(&try_map_language(&language)?)?;

        Ok(Self { parser })
    }

    pub fn parse(&mut self, code: &str) -> Result<CodeTree> {
        let tree = self.parser.parse(code, None).context("No nodes found")?;

        let parsed_root_node = parse_node(tree.root_node(), code);
        let code_tree = CodeTree {
            ts_tree: tree,
            root_node: parsed_root_node,
        };
        Ok(code_tree)
    }
}

fn parse_node(node: Node, code: &str) -> CodeNode {
    let mut children = vec![];

    // Assume that unnamed nodes have no children
    // It's a rought world
    let end_byte = node
        .child(0)
        .map(|n| n.start_byte())
        .unwrap_or_else(|| node.end_byte());

    for child in node.named_children(&mut node.walk()) {
        let child_node = parse_node(child, code);
        children.push(child_node);
    }

    CodeNode {
        // ts_node: node,
        grammar_name: node.grammar_name().to_string(),
        kind: node.kind().to_string(),
        name: code[node.start_byte()..end_byte].to_string(),
        children,
    }
}
