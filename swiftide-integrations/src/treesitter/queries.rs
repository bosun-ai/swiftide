// https://github.com/tree-sitter/tree-sitter-ruby/blob/master/queries/tags.scm
pub mod ruby {
    pub const DEFS: &str = r"
(
  [
    (method
      name: (_) @name)
    (singleton_method
      name: (_) @name)
  ]
)

(alias
  name: (_) @name)

(setter
  (identifier) @ignore)

(
  [
    (class
      name: [
        (constant) @name
        (scope_resolution
          name: (_) @name)
      ]) 
    (singleton_class
      value: [
        (constant) @name
        (scope_resolution
          name: (_) @name)
      ])
  ]
)

(
  (module
    name: [
      (constant) @name
      (scope_resolution
        name: (_) @name)
    ])
)
";

    pub const REFS: &str = r#"
(call method: (identifier) @name)

(
  [(identifier) (constant)] @name
  (#is-not? local)
  (#not-match? @name "^(lambda|load|require|require_relative|__FILE__|__LINE__)$")
)
"#;
}

// https://github.com/tree-sitter/tree-sitter-python/blob/master/queries/tags.scm
pub mod python {
    pub const DEFS: &str = r#"
            (class_definition
                name: (identifier) @name)

            (
            (function_definition
                name: (identifier) @name)
            (#not-eq? @name "__init__")
            )

        "#;

    pub const REFS: &str = "

            (call
            function: [
                (identifier) @name
                (attribute
                    attribute: (identifier))
            ])
        ";
}

// https://github.com/tree-sitter/tree-sitter-typescript/blob/master/queries/tags.scm
pub mod typescript {
    pub const DEFS: &str = r#"
            (function_signature
                name: (identifier) @name)

            (method_signature
                name: (property_identifier) @name)

            (abstract_method_signature
                name: (property_identifier) @name)

            (abstract_class_declaration
                name: (type_identifier) @name)

            (module
                name: (identifier) @name)

            (interface_declaration
                name: (type_identifier) @name)

            (
            (method_definition
                name: (property_identifier) @name)
            (#not-eq? @name "constructor")
            )

            (
            [
                (class
                name: (_) @name)
                (class_declaration
                name: (_) @name)
            ] 
            )

            (
            [
                (function_expression
                name: (identifier) @name)
                (function_declaration
                name: (identifier) @name)
                (generator_function
                name: (identifier) @name)
                (generator_function_declaration
                name: (identifier) @name)
            ] 
            )

            (
            (lexical_declaration
                (variable_declarator
                name: (identifier) @name
                value: [(arrow_function) (function_expression)]))
            )

            (
            (variable_declaration
                (variable_declarator
                name: (identifier) @name
                value: [(arrow_function) (function_expression)]))
            )
        "#;

    pub const REFS: &str = r#"
            (type_annotation
                (type_identifier) @name)

            (new_expression
                constructor: (identifier) @name)
            (
            (call_expression
                function: (identifier) @name) 
            (#not-match? @name "^(require)$")
            )

            (call_expression
            function: (member_expression
                property: (property_identifier) @name)
            arguments: (_))
        "#;
}

// https://github.com/tree-sitter/tree-sitter-javascript/blob/master/queries/tags.scm
pub mod javascript {
    pub const DEFS: &str = r#"
        (
        (method_definition
            name: (property_identifier) @name)
        (#not-eq? @name "constructor")
        )

        (
        [
            (class
            name: (_) @name)
            (class_declaration
            name: (_) @name)
        ] 
        )

        (
        [
            (function_expression
            name: (identifier) @name)
            (function_declaration
            name: (identifier) @name)
            (generator_function
            name: (identifier) @name)
            (generator_function_declaration
            name: (identifier) @name)
        ] 
        )

        (
        (lexical_declaration
            (variable_declarator
            name: (identifier) @name
            value: [(arrow_function) (function_expression)]) @definition.function)
        )

        (
        (variable_declaration
            (variable_declarator
            name: (identifier) @name
            value: [(arrow_function) (function_expression)]) @definition.function)
        )

        (assignment_expression
        left: [
            (identifier) @name
            (member_expression
            property: (property_identifier) @name)
        ]
        right: [(arrow_function) (function_expression)]
        ) 

        (pair
        key: (property_identifier) @name
        value: [(arrow_function) (function_expression)])

        "#;

    pub const REFS: &str = r#"
        (
        (call_expression
            function: (identifier) @name) 
        (#not-match? @name "^(require)$")
        )

        (call_expression
        function: (member_expression
            property: (property_identifier) @name)
        arguments: (_))

        (new_expression
        constructor: (_) @name)

        (export_statement value: (assignment_expression left: (identifier) @name right: ([
        (number)
        (string)
        (identifier)
        (undefined)
        (null)
        (new_expression)
        (binary_expression)
        (call_expression)
        ]))) 
    "#;
}

// https://github.com/tree-sitter/tree-sitter-rust/blob/master/queries/tags.scm
pub mod rust {
    pub const DEFS: &str = "
            (struct_item
                name: (type_identifier) @name)

            (enum_item
                name: (type_identifier) @name)

            (union_item
                name: (type_identifier) @name)

            (type_item
                name: (type_identifier) @name)

            (declaration_list
                (function_item
                    name: (identifier) @name))

            (function_item
                name: (identifier) @name)

            (trait_item
                name: (type_identifier) @name)

            (mod_item
                name: (identifier) @name)

            (macro_definition
                name: (identifier) @name)
        ";

    pub const REFS: &str = "
            (call_expression
                function: (identifier) @name)

            (call_expression
                function: (field_expression
                    field: (field_identifier) @name))

            (macro_invocation
                macro: (identifier) @name)
        ";
}

// https://github.com/tree-sitter/tree-sitter-java/blob/master/queries/tags.scm
pub mod java {
    pub const DEFS: &str = "
           (class_declaration
                name: (identifier) @name)

           (enum_declaration
                name: (identifier) @name)

            (method_declaration
                name: (identifier) @name)

            (interface_declaration
                name: (identifier) @name)

            (type_list
                (type_identifier) @name)

            (superclass (type_identifier) @name)";
    pub const REFS: &str = "
            (method_invocation
                name: (identifier) @name
                arguments: (argument_list))

            (object_creation_expression
                type: (type_identifier) @name)";
}

pub mod go {
    pub const DEFS: &str = r"
    (function_declaration
    name: (identifier) @name)

    (method_declaration
    name: (field_identifier) @name)

    (type_declaration (type_spec name: (type_identifier) @name type: (interface_type)))

    (type_declaration (type_spec name: (type_identifier) @name type: (struct_type)))

    (import_declaration (import_spec) @name)

    (var_declaration (var_spec name: (identifier) @name))

    (const_declaration (const_spec name: (identifier) @name))

            ";

    pub const REFS: &str = r#"
    (call_expression
    function: [
        (identifier) @name
        (parenthesized_expression (identifier) @name)
        (selector_expression field: (field_identifier) @name)
        (parenthesized_expression (selector_expression field: (field_identifier) @name))
    ])

    (type_spec
    name: (type_identifier) @name) 

    (package_clause "package" (package_identifier) @name)
    (type_identifier) @name 
            "#;
}

pub mod solidity {
    pub const DEFS: &str = r"
    (function_definition
    name: (identifier) @name)

    (source_file
        (function_definition
            name: (identifier) @name))

    (contract_declaration
    name: (identifier) @name) 

    (interface_declaration
    name: (identifier) @name)

    (library_declaration
    name: (identifier) @name)

    (struct_declaration name: (identifier) @name)
    (enum_declaration name: (identifier) @name)
    (event_definition name: (identifier) @name)
    ";

    pub const REFS: &str = r"
    (call_expression (expression (identifier)) @name )

    (call_expression
        (expression (member_expression
            property: (_) @name )))

    (emit_statement name: (_) @name)


    (inheritance_specifier
        ancestor: (user_defined_type (_) @name . ))


    (import_directive
    import_name: (_) @name )
    ";
}
