use crate::treesitter::refs_defs::types::{Definition, DefinitionType, File};
use anyhow::{Context, Result};
use tree_sitter::{Node, Parser, TreeCursor};

pub struct DefinitionIdentifier {
    language: tree_sitter::Language,
}

impl DefinitionIdentifier {
    pub fn new(language: tree_sitter::Language) -> Self {
        Self { language }
    }

    pub fn identify_definitions(&self, file_name: &str, code: &str) -> Result<File> {
        let mut parser = Parser::new();
        parser.set_language(&self.language)?;
        let tree = parser.parse(code, None).context("No nodes found")?;
        let root_node = tree.root_node();

        if root_node.has_error() {
            anyhow::bail!("Root node has invalid syntax");
        }

        let mut file = File::new(
            file_name.to_string(),
            file_name.to_string(), // You might want to pass the full path separately
            "Rust".to_string(),
        );

        let mut cursor = root_node.walk();
        self.process_node(&mut cursor, code, &mut file, None);

        Ok(file)
    }

    fn process_node(
        &self,
        cursor: &mut TreeCursor,
        source: &str,
        file: &mut File,
        parent_id: Option<String>,
    ) {
        let node = cursor.node();

        if let Some(definition) = self.node_to_definition(node, source) {
            let def_id = definition.id.clone();
            file.add_definition(definition);

            if let Some(ref parent_id) = parent_id {
                if let Some(parent_def) = file
                    .definitions
                    .iter_mut()
                    .find(|d| d.id == parent_id.as_str())
                {
                    parent_def.add_contained_definition(def_id.clone());
                }
            }

            let mut next_cursor = cursor.clone();
            if next_cursor.goto_first_child() {
                self.process_node(&mut next_cursor, source, file, Some(def_id));
            }
        } else {
            let mut next_cursor = cursor.clone();
            if next_cursor.goto_first_child() {
                self.process_node(&mut next_cursor, source, file, parent_id.clone());
            }
        }

        if cursor.goto_next_sibling() {
            self.process_node(cursor, source, file, parent_id);
        }
    }

    fn node_to_definition(&self, node: Node, source: &str) -> Option<Definition> {
        match node.kind() {
            "struct_item" => self.create_definition(node, source, "class".to_string()),
            "enum_item" => self.create_definition(node, source, "class".to_string()),
            "trait_item" => self.create_definition(node, source, "class".to_string()),
            "impl_item" => self.create_definition(node, source, "class".to_string()),
            "function_item" => self.create_definition(node, source, "function".to_string()),
            "function_signature_item" => {
                self.create_definition(node, source, "function".to_string())
            }
            "mod_item" => self.create_definition(node, source, "module".to_string()),
            _ => None,
        }
    }

    fn create_definition(
        &self,
        node: Node,
        source: &str,
        def_type: DefinitionType,
    ) -> Option<Definition> {
        let name_node = node.child_by_field_name("name")?;
        let name = name_node.utf8_text(source.as_bytes()).ok()?;

        Some(Definition::new(
            format!("def_{}", node.id()),
            name.to_string(),
            def_type,
            true, // All these types can contain other definitions
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_definition_identifier() {
        let source_code = r#"
mod my_module {
    pub struct MyStruct {
        field: i32,
    }

    impl MyStruct {
        fn new() -> Self {
            MyStruct { field: 0 }
        }
    }

    pub fn module_function() -> i32 {
        42
    }

    pub trait MyTrait {
        fn trait_method(&self);
    }

    pub enum MyEnum {
        VariantA,
        VariantB(i32),
    }
}

fn main() {
    println!("Hello, world!");
}
"#;

        let language = tree_sitter_rust::language();
        let identifier = DefinitionIdentifier::new(language);

        let file = identifier
            .identify_definitions("test_file.rs", source_code)
            .unwrap();

        // Check if all expected definitions are present
        let expected_definitions = vec![
            ("my_module", "module"),
            ("MyStruct", "class"),
            ("new", "function"),
            ("module_function", "function"),
            ("MyTrait", "class"),
            ("trait_method", "function"),
            ("MyEnum", "class"),
            ("main", "function"),
        ];

        for (name, def_type) in expected_definitions.clone() {
            let entry = file.definitions.iter().find(|def| def.name == *name);

            assert!(
                entry.is_some(),
                "Expected definition not found: {} ({:?}), all entries: {:?}",
                name,
                def_type,
                file.definitions
            );

            let entry = entry.unwrap();

            assert!(
                entry.definition_type == def_type,
                "Expected definition has wrong type: {} (has {:?}, expected {:?}), all entries: {:?}",
                name,
                entry.definition_type,
                def_type,
                file.definitions
            );
        }

        // Check hierarchical structure
        let module_def = file
            .definitions
            .iter()
            .find(|d| d.name == "my_module")
            .unwrap();
        let module_children: HashSet<_> = module_def
            .contained_definitions
            .iter()
            .map(|id| {
                file.definitions
                    .iter()
                    .find(|d| d.id == *id)
                    .unwrap()
                    .name
                    .to_string()
            })
            .collect();

        let expected_module_children: HashSet<_> = file
            .definitions
            .iter()
            .filter(|d| {
                d.name == "MyStruct"
                    || d.name == "module_function"
                    || d.name == "MyTrait"
                    || d.name == "MyEnum"
            })
            .map(|d| d.name.clone())
            .collect();

        assert_eq!(
            module_children, expected_module_children,
            "Module's contained definitions do not match expected"
        );

        // Check that main function is not in the module
        assert!(
            !module_children.contains(
                &file
                    .definitions
                    .iter()
                    .find(|d| d.name == "main")
                    .unwrap()
                    .id
            ),
            "Main function should not be in the module"
        );

        // Check that the impl block contains the 'new' function
        let impl_def = file
            .definitions
            .iter()
            .find(|d| {
                d.name == "MyStruct"
                    && d.contained_definitions.iter().any(|id| {
                        file.definitions
                            .iter()
                            .any(|d2| d2.id == *id && d2.name == "new")
                    })
            })
            .unwrap();

        assert!(
            !impl_def.contained_definitions.is_empty(),
            "Impl block should contain the 'new' function"
        );
    }
}
