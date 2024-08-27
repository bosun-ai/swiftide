use std::sync::Arc;

use derive_builder::Builder;
use lancedb::arrow::arrow_schema::{DataType, Field, Schema};
use swiftide_core::indexing::EmbeddedField;

#[derive(Builder, Clone)]
#[builder(setter(into))]
pub struct LanceDB {
    client: Arc<lancedb::Connection>,
    #[builder(default = "self.default_schema_from_fields()")]
    schema: Arc<Schema>,

    table_name: String,
    vector_size: i32,
    uri: String,
    #[builder(private, default)]
    fields: Vec<FieldConfig>,
}

impl std::fmt::Debug for LanceDB {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("LanceDB")
            .field("schema", &self.schema)
            .finish()
    }
}

impl LanceDB {
    pub fn builder() -> LanceDBBuilder {
        LanceDBBuilder::default()
    }
}

impl LanceDBBuilder {
    pub fn with_vector(&mut self, config: impl Into<VectorConfig>) -> &mut Self {
        if self.fields.is_none() {
            self.fields(Vec::default());
        }

        self.fields
            .as_mut()
            .expect("Fields should be initialized")
            .push(FieldConfig::Vector(config.into()));

        self
    }

    pub fn with_sparse_vector(&mut self, config: impl Into<SparseVectorConfig>) -> &mut Self {
        if self.fields.is_none() {
            self.fields(Vec::default());
        }
        self.fields
            .as_mut()
            .expect("Fields should be initialized")
            .push(FieldConfig::SparseVector(config.into()));

        self
    }

    fn default_schema_from_fields(&self) -> Arc<Schema> {
        let mut fields = Vec::new();
        let vector_size = self.vector_size;

        for ref field in self.fields.clone().unwrap_or_default() {
            match field {
                FieldConfig::Vector(config) => {
                    let vector_size = config.vector_size.or(vector_size).expect(
                        "Vector size should be set either in the field or in the LanceDB builder",
                    );

                    fields.push(Field::new(
                        config.embedded_field.to_string(),
                        DataType::FixedSizeList(
                            Arc::new(Field::new("item", DataType::Float32, false)),
                            vector_size,
                        ),
                        false,
                    ));
                }
                FieldConfig::SparseVector(config) => {
                    fields.push(Field::new(
                        format!("{}_sparse", config.embedded_field),
                        DataType::Float64,
                        true,
                    ));
                }
            }
        }
        Arc::new(Schema::new(fields))
    }
}

#[derive(Clone)]
pub enum FieldConfig {
    Vector(VectorConfig),
    SparseVector(SparseVectorConfig),
}

#[derive(Clone)]
pub struct VectorConfig {
    embedded_field: EmbeddedField,
    vector_size: Option<i32>,
}

impl Into<VectorConfig> for EmbeddedField {
    fn into(self) -> VectorConfig {
        VectorConfig {
            embedded_field: self,
            vector_size: None,
        }
    }
}

#[derive(Clone)]
pub struct SparseVectorConfig {
    embedded_field: EmbeddedField,
}

impl Into<SparseVectorConfig> for EmbeddedField {
    fn into(self) -> SparseVectorConfig {
        SparseVectorConfig {
            embedded_field: self,
        }
    }
}
