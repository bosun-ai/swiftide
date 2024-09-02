use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct File {
    pub name: String,
    pub path: String,
    pub language: String,
    pub definitions: Vec<Definition>,
    pub references: Vec<Reference>,
}

#[derive(Debug, Clone)]
pub struct Definition {
    pub id: String,
    pub name: String,
    pub definition_type: DefinitionType,
    pub alias: Option<String>,
    pub is_scope: bool,
    pub contained_definitions: Vec<String>, // IDs of contained definitions
    pub contained_references: Vec<String>,  // IDs of contained references
    pub language_specific_properties: HashMap<String, String>,
}

pub type DefinitionType = String;

#[derive(Debug, Clone)]
pub struct Reference {
    pub id: String,
    pub reference_type: ReferenceType,
    pub location: Location,
    pub refers_to: ReferenceTarget,
}

pub type ReferenceType = String;

#[derive(Debug, Clone)]
pub struct Location {
    pub line: usize,
    pub line_end: usize,
    pub column: usize,
    pub column_end: usize,
}

#[derive(Debug, Clone)]
pub enum ReferenceTarget {
    Internal(String), // ID of the definition within the same file
    External {
        name: String,
        presumed_type: DefinitionType,
    },
    Unresolved(Vec<String>), // Lexical scope of the reference
}

impl File {
    pub fn new(name: String, path: String, language: String) -> Self {
        File {
            name,
            path,
            language,
            definitions: Vec::new(),
            references: Vec::new(),
        }
    }

    pub fn add_definition(&mut self, definition: Definition) {
        self.definitions.push(definition);
    }

    pub fn add_reference(&mut self, reference: Reference) {
        self.references.push(reference);
    }
}

impl Definition {
    pub fn new(
        id: String,
        name: String,
        definition_type: DefinitionType,
        alias: Option<String>,
        is_scope: bool,
    ) -> Self {
        Definition {
            id,
            name,
            definition_type,
            alias,
            is_scope,
            contained_definitions: Vec::new(),
            contained_references: Vec::new(),
            language_specific_properties: HashMap::new(),
        }
    }

    pub fn add_contained_definition(&mut self, definition_id: String) {
        self.contained_definitions.push(definition_id);
    }

    pub fn add_contained_reference(&mut self, reference_id: String) {
        self.contained_references.push(reference_id);
    }
}

impl Reference {
    pub fn new(
        id: String,
        reference_type: ReferenceType,
        location: Location,
        refers_to: ReferenceTarget,
    ) -> Self {
        Reference {
            id,
            reference_type,
            location,
            refers_to,
        }
    }
}
