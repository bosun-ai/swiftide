//! Code parsing
//!
//! Extracts typed semantics from code.
#![allow(dead_code)]
use itertools::Itertools;
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator as _, Tree};

use anyhow::{Context as _, Result};
use std::collections::HashSet;

use crate::treesitter::queries::{go, java, javascript, python, ruby, rust, solidity, typescript};

use super::SupportedLanguages;

#[derive(Debug)]
pub struct CodeParser {
    language: SupportedLanguages,
}

impl CodeParser {
    pub fn from_language(language: SupportedLanguages) -> Self {
        Self { language }
    }

    /// Parses code and returns a `CodeTree`
    ///
    /// Tree-sitter is pretty lenient and will parse invalid code. I.e. if the code is invalid,
    /// queries might fail and return no results.
    ///
    /// This is good as it makes this safe to use for chunked code as well.
    ///
    /// # Errors
    ///
    /// Errors if the language is not support or if the tree cannot be parsed
    pub fn parse<'a>(&self, code: &'a str) -> Result<CodeTree<'a>> {
        let mut parser = Parser::new();
        parser.set_language(&self.language.into())?;
        let ts_tree = parser.parse(code, None).context("No nodes found")?;

        Ok(CodeTree {
            ts_tree,
            code,
            language: self.language,
        })
    }
}

/// A code tree is a queryable representation of code
pub struct CodeTree<'a> {
    ts_tree: Tree,
    code: &'a str,
    language: SupportedLanguages,
}

pub struct ReferencesAndDefinitions {
    pub references: Vec<String>,
    pub definitions: Vec<String>,
}

impl CodeTree<'_> {
    /// Queries for references and definitions in the code. It returns a unique list of non-local
    /// references, and local definitions.
    ///
    /// # Errors
    ///
    /// Errors if the query is invalid or fails
    pub fn references_and_definitions(&self) -> Result<ReferencesAndDefinitions> {
        let (defs, refs) = ts_queries_for_language(self.language);

        let defs_query = Query::new(&self.language.into(), defs)?;
        let refs_query = Query::new(&self.language.into(), refs)?;

        let defs = self.ts_query_for_matches(&defs_query)?;
        let refs = self.ts_query_for_matches(&refs_query)?;

        Ok(ReferencesAndDefinitions {
            // Remove any self references
            references: refs
                .into_iter()
                .filter(|r| !defs.contains(r))
                .sorted()
                .collect(),
            definitions: defs.into_iter().sorted().collect(),
        })
    }

    /// Given a `tree-sitter` query, searches the code and returns a list of matching symbols
    fn ts_query_for_matches(&self, query: &Query) -> Result<HashSet<String>> {
        let mut cursor = QueryCursor::new();

        cursor
            .matches(query, self.ts_tree.root_node(), self.code.as_bytes())
            .map_deref(|m| {
                m.captures
                    .iter()
                    .map(|c| {
                        Ok(c.node
                            .utf8_text(self.code.as_bytes())
                            .context("Failed to parse node")?
                            .to_string())
                    })
                    .collect::<Result<Vec<_>>>()
                    .map(|s| s.join(""))
            })
            .collect::<Result<HashSet<_>>>()
    }
}

fn ts_queries_for_language(language: SupportedLanguages) -> (&'static str, &'static str) {
    use SupportedLanguages::{
        Cpp, Go, Java, Javascript, Python, Ruby, Rust, Solidity, Typescript, C,
    };

    match language {
        Rust => (rust::DEFS, rust::REFS),
        Python => (python::DEFS, python::REFS),
        // The univocal proof that TS is just a linter
        Typescript => (typescript::DEFS, typescript::REFS),
        Javascript => (javascript::DEFS, javascript::REFS),
        Ruby => (ruby::DEFS, ruby::REFS),
        Java => (java::DEFS, java::REFS),
        Go => (go::DEFS, go::REFS),
        Solidity => (solidity::DEFS, solidity::REFS),
        C | Cpp => unimplemented!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parsing_on_rust() {
        let parser = CodeParser::from_language(SupportedLanguages::Rust);
        let code = r#"
        use std::io;

        fn main() {
            println!("Hello, world!");
        }
        "#;
        let tree = parser.parse(code).unwrap();
        let result = tree.references_and_definitions().unwrap();
        assert_eq!(result.references, vec!["println"]);

        assert_eq!(result.definitions, vec!["main"]);
    }

    #[test]
    fn test_parsing_on_solidity() {
        let parser = CodeParser::from_language(SupportedLanguages::Solidity);
        let code = r"
        pragma solidity ^0.8.0;

        contract MyContract {
            function myFunction() public {
                emit MyEvent();
            }
        }
        ";
        let tree = parser.parse(code).unwrap();
        let result = tree.references_and_definitions().unwrap();
        assert_eq!(result.references, vec!["MyEvent"]);
        assert_eq!(result.definitions, vec!["MyContract", "myFunction"]);
    }

    #[test]
    fn test_parsing_on_ruby() {
        let parser = CodeParser::from_language(SupportedLanguages::Ruby);
        let code = r#"
        class A < Inheritance
          include ActuallyAlsoInheritance

          def a
            puts "A"
          end
        end
        "#;

        let tree = parser.parse(code).unwrap();
        let result = tree.references_and_definitions().unwrap();
        assert_eq!(
            result.references,
            ["ActuallyAlsoInheritance", "Inheritance", "include", "puts",]
        );

        assert_eq!(result.definitions, ["A", "a"]);
    }

    #[test]
    fn test_parsing_python() {
        // test with a python class and list comprehension
        let parser = CodeParser::from_language(SupportedLanguages::Python);
        let code = r#"
        class A:
            def __init__(self):
                self.a = [x for x in range(10)]

        def hello_world():
            print("Hello, world!")
        "#;
        let tree = parser.parse(code).unwrap();
        let result = tree.references_and_definitions().unwrap();
        assert_eq!(result.references, ["print", "range"]);
        assert_eq!(result.definitions, vec!["A", "hello_world"]);
    }

    #[test]
    fn test_parsing_on_typescript() {
        let parser = CodeParser::from_language(SupportedLanguages::Typescript);
        let code = r#"
        function Test() {
            console.log("Hello, TypeScript!");
            otherThing();
        }

        class MyClass {
            constructor() {
                let local = 5;
                this.myMethod();
            }

            myMethod() {
                console.log("Hello, TypeScript!");
            }
        }
        "#;

        let tree = parser.parse(code).unwrap();
        let result = tree.references_and_definitions().unwrap();
        assert_eq!(result.definitions, vec!["MyClass", "Test", "myMethod"]);
        assert_eq!(result.references, vec!["log", "otherThing"]);
    }

    #[test]
    fn test_parsing_on_javascript() {
        let parser = CodeParser::from_language(SupportedLanguages::Javascript);
        let code = r#"
        function Test() {
            console.log("Hello, JavaScript!");
            otherThing();
        }
        class MyClass {
            constructor() {
                let local = 5;
                this.myMethod();
            }
            myMethod() {
                console.log("Hello, JavaScript!");
            }
        }
        "#;
        let tree = parser.parse(code).unwrap();
        let result = tree.references_and_definitions().unwrap();
        assert_eq!(result.definitions, vec!["MyClass", "Test", "myMethod"]);
        assert_eq!(result.references, vec!["log", "otherThing"]);
    }

    #[test]
    fn test_parsing_on_java() {
        let parser = CodeParser::from_language(SupportedLanguages::Java);
        let code = r#"
        public class Hello {
            public static void main(String[] args) {
                System.out.printf("Hello %s!%n", args[0]);
            }
        }
        "#;
        let tree = parser.parse(code).unwrap();
        let result = tree.references_and_definitions().unwrap();
        assert_eq!(result.definitions, vec!["Hello", "main"]);
        assert_eq!(result.references, vec!["printf"]);
    }

    #[test]
    fn test_parsing_on_java_enum() {
        let parser = CodeParser::from_language(SupportedLanguages::Java);
        let code = r"
        enum Material {
            DENIM,
            CANVAS,
            SPANDEX_3_PERCENT
        }

        class Person {


          Person(string name) {
            this.name = name;

            this.pants = new Pants<Pocket>();
          }

          String getName() {
            a = this.name;
            b = new one.two.Three();
            c = Material.DENIM;
          }
        }
        ";
        let tree = parser.parse(code).unwrap();
        let result = tree.references_and_definitions().unwrap();
        assert_eq!(result.definitions, vec!["Material", "Person", "getName"]);
        assert!(result.references.is_empty());
    }

    #[test]
    fn test_parsing_go() {
        let parser = CodeParser::from_language(SupportedLanguages::Go);
        // hello world go with struct
        let code = r"
        package main

        type Person struct {
            name string
            age int
        }

        func main() {
            p := Person{name: 'John', age: 30}
            fmt.Println(p)
        }
        ";

        let tree = parser.parse(code).unwrap();
        let result = tree.references_and_definitions().unwrap();
        assert_eq!(result.references, vec!["Println", "int", "string"]);
        assert_eq!(result.definitions, vec!["Person", "main"]);
    }
}
