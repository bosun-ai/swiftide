use std::pin::Pin;
use std::sync::Arc;

use anyhow::Context as _;
use anyhow::Result;
use connection_pool::LanceDBConnectionPool;
use connection_pool::LanceDBPoolManager;
use deadpool::managed::Object;
use derive_builder::Builder;
use lancedb::arrow::arrow_schema::{DataType, Field, Schema};
use swiftide_core::indexing::EmbeddedField;
pub mod connection_pool;
pub mod persist;

#[derive(Builder, Clone)]
#[builder(setter(into, strip_option), build_fn(error = "anyhow::Error"))]
pub struct LanceDB {
    /// Connection pool for LanceDB
    /// By default will use settings provided when creating the LanceDB instance.
    #[builder(default = "self.default_connection_pool()?")]
    connection_pool: Arc<LanceDBConnectionPool>,

    /// Set the URI for the LanceDB instance. Required unless a connection pool is provided.
    uri: Option<String>,
    #[builder(default = "Some(10)")]
    /// The maximum number of connections to LanceDB, defaults to 10.
    pool_size: Option<usize>,

    /// API key for LanceDB
    api_key: Option<String>,
    /// Region for LanceDB
    region: Option<String>,
    /// Storage options
    storage_options: Vec<(String, String)>,

    #[builder(private, default = "self.default_schema_from_fields()")]
    schema: Arc<Schema>,

    /// The name of the table to store the data
    /// By default will use `swiftide`
    #[builder(default = "\"swiftide\".into()")]
    table_name: String,

    /// Default sizes of vectors. Vectors can also be of different
    /// sizes by specifying the size in the vector configuration.
    vector_size: Option<i32>,

    /// Batch size for storing nodes in LanceDB. Default is 256.
    #[builder(default = "256")]
    batch_size: usize,

    /// Field configuration for LanceDB, will result in the eventual schema.
    ///
    /// Supports multiple field types, see [`FieldConfig`] for more details.
    #[builder(default)]
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

    /// Get a connection to LanceDB from the connection pool
    ///
    /// # Errors
    ///
    /// Returns an error if the connection cannot be retrieved.
    pub async fn get_connection(&self) -> Result<Object<LanceDBPoolManager>> {
        Box::pin(self.connection_pool.get())
            .await
            .map_err(|e| anyhow::anyhow!(e))
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

    pub fn with_metadata(&mut self, config: impl Into<MetadataConfig>) -> &mut Self {
        if self.fields.is_none() {
            self.fields(Vec::default());
        }
        self.fields
            .as_mut()
            .expect("Fields should be initialized")
            .push(FieldConfig::Metadata(config.into()));
        self
    }

    fn default_schema_from_fields(&self) -> Arc<Schema> {
        let mut fields = Vec::new();
        let vector_size = self.vector_size;

        for ref field in self.fields.clone().unwrap_or_default() {
            match field {
                FieldConfig::Vector(config) => {
                    let vector_size = config.vector_size.or(vector_size.flatten()).expect(
                        "Vector size should be set either in the field or in the LanceDB builder",
                    );

                    fields.push(Field::new(
                        config.field_name(),
                        DataType::FixedSizeList(
                            Arc::new(Field::new("item", DataType::Float32, false)),
                            vector_size,
                        ),
                        false,
                    ));
                }
                FieldConfig::Chunk => {
                    fields.push(Field::new("chunk", DataType::Utf8, true));
                }
                FieldConfig::Metadata(config) => {
                    fields.push(Field::new(&config.field, DataType::Utf8, true));
                }
            }
        }
        Arc::new(Schema::new(fields))
    }

    fn default_connection_pool(&self) -> Result<Arc<LanceDBConnectionPool>> {
        let mgr = LanceDBPoolManager::builder()
            .uri(self.uri.clone().flatten().context("URI should be set")?)
            .api_key(self.api_key.clone().flatten())
            .region(self.region.clone().flatten())
            .storage_options(self.storage_options.clone().unwrap_or_default())
            .build()?;

        LanceDBConnectionPool::builder(mgr)
            .max_size(
                self.pool_size
                    .flatten()
                    .context("Pool size should be set")?,
            )
            .build()
            .map(Arc::new)
            .map_err(Into::into)
    }
}

#[derive(Clone)]
pub enum FieldConfig {
    Vector(VectorConfig),
    Metadata(MetadataConfig),
    Chunk,
}

#[derive(Clone)]
pub struct VectorConfig {
    embedded_field: EmbeddedField,
    vector_size: Option<i32>,
}

impl VectorConfig {
    pub fn field_name(&self) -> String {
        format!(
            "vector_{}",
            normalize_field_name(&self.embedded_field.to_string())
        )
    }
}

impl From<EmbeddedField> for VectorConfig {
    fn from(val: EmbeddedField) -> Self {
        VectorConfig {
            embedded_field: val,
            vector_size: None,
        }
    }
}

#[derive(Clone)]
pub struct SparseVectorConfig {
    embedded_field: EmbeddedField,
}

impl From<EmbeddedField> for SparseVectorConfig {
    fn from(val: EmbeddedField) -> Self {
        SparseVectorConfig {
            embedded_field: val,
        }
    }
}

#[derive(Clone)]
pub struct MetadataConfig {
    field: String,
}

impl<T: AsRef<str>> From<T> for MetadataConfig {
    fn from(val: T) -> Self {
        MetadataConfig {
            field: normalize_field_name(val.as_ref()),
        }
    }
}

fn normalize_field_name(field: &str) -> String {
    field
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric(), "_")
}
