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
pub mod retrieve;

/// `LanceDB` is a columnar database that separates data and compute.
///
/// This enables local, embedded databases, or storing in a cloud storage.
///
/// See examples for more information.
///
/// Implements `Persist` and `Retrieve`.
///
/// If you want to store / retrieve metadata in Lance, the columns can be defined with
/// `with_metadata`.
///
/// Note: For querying large tables you manually need to create an index. You can get an
/// active connection via `get_connection`.
///
/// # Example
///
/// ```no_run
/// # use swiftide_integrations::lancedb::{LanceDB};
/// # use swiftide_core::indexing::EmbeddedField;
/// LanceDB::builder()
/// .uri("/my/lancedb")
/// .vector_size(1536)
/// .with_vector(EmbeddedField::Combined)
/// .with_metadata("Metadata field to also store")
/// .table_name("swiftide_test")
/// .build()
/// .unwrap();
#[derive(Builder, Clone)]
#[builder(setter(into, strip_option), build_fn(error = "anyhow::Error"))]
#[allow(dead_code)]
pub struct LanceDB {
    /// Connection pool for `LanceDB`
    /// By default will use settings provided when creating the instance.
    #[builder(default = "self.default_connection_pool()?")]
    connection_pool: Arc<LanceDBConnectionPool>,

    /// Set the URI. Required unless a connection pool is provided.
    uri: Option<String>,
    /// The maximum number of connections, defaults to 10.
    #[builder(default = "Some(10)")]
    pool_size: Option<usize>,

    /// Optional API key
    #[builder(default)]
    api_key: Option<String>,
    /// Optional Region
    #[builder(default)]
    region: Option<String>,
    /// Storage options
    #[builder(default)]
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

    /// Batch size for storing nodes in `LanceDB`. Default is 256.
    #[builder(default = "256")]
    batch_size: usize,

    /// Field configuration for `LanceDB`, will result in the eventual schema.
    ///
    /// Supports multiple field types, see [`FieldConfig`] for more details.
    #[builder(default = "self.default_fields()")]
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

    /// Get a connection to `LanceDB` from the connection pool
    ///
    /// # Errors
    ///
    /// Returns an error if the connection cannot be retrieved.
    pub async fn get_connection(&self) -> Result<Object<LanceDBPoolManager>> {
        Box::pin(self.connection_pool.get())
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// Opens the lancedb table
    ///
    /// # Errors
    ///
    /// Returns an error if the table cannot be opened or the connection cannot be acquired.
    pub async fn open_table(&self) -> Result<lancedb::Table> {
        let conn = self.get_connection().await?;
        conn.open_table(&self.table_name)
            .execute()
            .await
            .context("Failed to open table")
    }
}

impl LanceDBBuilder {
    #[allow(clippy::missing_panics_doc)]
    pub fn with_vector(&mut self, config: impl Into<VectorConfig>) -> &mut Self {
        if self.fields.is_none() {
            self.fields(self.default_fields());
        }

        self.fields
            .as_mut()
            .unwrap()
            .push(FieldConfig::Vector(config.into()));

        self
    }

    #[allow(clippy::missing_panics_doc)]
    pub fn with_metadata(&mut self, config: impl Into<MetadataConfig>) -> &mut Self {
        if self.fields.is_none() {
            self.fields(self.default_fields());
        }
        self.fields
            .as_mut()
            .unwrap()
            .push(FieldConfig::Metadata(config.into()));
        self
    }

    #[allow(clippy::unused_self)]
    fn default_fields(&self) -> Vec<FieldConfig> {
        vec![FieldConfig::ID, FieldConfig::Chunk]
    }

    fn default_schema_from_fields(&self) -> Arc<Schema> {
        let mut fields = Vec::new();
        let vector_size = self.vector_size;

        for field in self.fields.as_deref().unwrap_or(&self.default_fields()) {
            match field {
                FieldConfig::Vector(config) => {
                    let vector_size = config.vector_size.or(vector_size.flatten()).expect(
                        "Vector size should be set either in the field or in the LanceDB builder",
                    );

                    fields.push(Field::new(
                        config.field_name(),
                        DataType::FixedSizeList(
                            Arc::new(Field::new("item", DataType::Float32, true)),
                            vector_size,
                        ),
                        true,
                    ));
                }
                FieldConfig::Chunk => {
                    fields.push(Field::new(field.field_name(), DataType::Utf8, false));
                }
                FieldConfig::Metadata(_) => {
                    fields.push(Field::new(field.field_name(), DataType::Utf8, true));
                }
                FieldConfig::ID => {
                    fields.push(Field::new(
                        field.field_name(),
                        DataType::FixedSizeList(
                            Arc::new(Field::new("item", DataType::UInt8, true)),
                            16,
                        ),
                        false,
                    ));
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
            .max_size(self.pool_size.flatten().unwrap_or(10))
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
    ID,
}

impl FieldConfig {
    pub fn field_name(&self) -> String {
        match self {
            FieldConfig::Vector(config) => config.field_name(),
            FieldConfig::Metadata(config) => config.field.clone(),
            FieldConfig::Chunk => "chunk".into(),
            FieldConfig::ID => "id".into(),
        }
    }
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
pub struct MetadataConfig {
    field: String,
    original_field: String,
}

impl<T: AsRef<str>> From<T> for MetadataConfig {
    fn from(val: T) -> Self {
        MetadataConfig {
            field: normalize_field_name(val.as_ref()),
            original_field: val.as_ref().to_string(),
        }
    }
}

pub(crate) fn normalize_field_name(field: &str) -> String {
    field
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric(), "_")
}
