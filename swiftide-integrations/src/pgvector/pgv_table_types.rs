//! `PostgreSQL` table schema and type conversion utilities for vector storage.
//!
//! Provides schema configuration and data type conversion functionality:
//! - Table schema generation with vector and metadata columns
//! - Field configuration for different vector embedding types
//! - HNSW index creation for similarity search optimization
//! - Bulk data preparation and SQL query generation
use crate::pgvector::PgVector;
use anyhow::{anyhow, Result};
use pgvector as ExtPgVector;
use regex::Regex;
use sqlx::postgres::PgArguments;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::BTreeMap;
use swiftide_core::indexing::{EmbeddedField, Node};
use tokio::time::sleep;

/// Configuration for vector embedding columns in the `PostgreSQL` table.
///
/// This struct defines how vector embeddings are stored and managed in the database,
/// mapping Swiftide's embedded fields to `PostgreSQL` vector columns.
#[derive(Clone, Debug)]
pub struct VectorConfig {
    embedded_field: EmbeddedField,
    pub field: String,
}

impl VectorConfig {
    pub fn new(embedded_field: &EmbeddedField) -> Self {
        Self {
            embedded_field: embedded_field.clone(),
            field: format!(
                "vector_{}",
                PgVector::normalize_field_name(&embedded_field.to_string()),
            ),
        }
    }
}

impl From<EmbeddedField> for VectorConfig {
    fn from(val: EmbeddedField) -> Self {
        Self::new(&val)
    }
}

/// Configuration for metadata fields in the `PostgreSQL` table.
///
/// Handles the mapping and storage of metadata fields, ensuring proper column naming
/// and type conversion for `PostgreSQL` compatibility.
#[derive(Clone, Debug)]
pub struct MetadataConfig {
    field: String,
    original_field: String,
}

impl MetadataConfig {
    pub fn new<T: Into<String>>(original_field: T) -> Self {
        let original = original_field.into();
        Self {
            field: format!("meta_{}", PgVector::normalize_field_name(&original)),
            original_field: original,
        }
    }
}

impl<T: AsRef<str>> From<T> for MetadataConfig {
    fn from(val: T) -> Self {
        Self::new(val.as_ref())
    }
}

/// Field configuration types supported in the `PostgreSQL` table schema.
///
/// Represents different field types that can be configured in the table schema,
/// including vector embeddings, metadata, and system fields.
#[derive(Clone, Debug)]
pub enum FieldConfig {
    /// `Vector` - Vector embedding field configuration
    Vector(VectorConfig),
    /// `Metadata` - Metadata field configuration
    Metadata(MetadataConfig),
    /// `Chunk` - Text content storage field
    Chunk,
    /// `ID` - Primary key field
    ID,
}

impl FieldConfig {
    pub fn field_name(&self) -> &str {
        match self {
            FieldConfig::Vector(config) => &config.field,
            FieldConfig::Metadata(config) => &config.field,
            FieldConfig::Chunk => "chunk",
            FieldConfig::ID => "id",
        }
    }
}

/// Internal structure for managing bulk upsert operations.
///
/// Collects and organizes data for efficient bulk insertions and updates,
/// grouping related fields for UNNEST-based operations.
struct BulkUpsertData<'a> {
    ids: Vec<sqlx::types::Uuid>,
    chunks: Vec<&'a str>,
    metadata_fields: Vec<Vec<serde_json::Value>>,
    vector_fields: Vec<Vec<ExtPgVector::Vector>>,
    field_mapping: FieldMapping<'a>,
}

struct FieldMapping<'a> {
    metadata_names: Vec<&'a str>,
    vector_names: Vec<&'a str>,
}

impl<'a> BulkUpsertData<'a> {
    fn new(fields: &'a [FieldConfig], size: usize) -> Self {
        let (metadata_names, vector_names): (Vec<&str>, Vec<&str>) = (
            fields
                .iter()
                .filter_map(|field| match field {
                    FieldConfig::Metadata(config) => Some(config.field.as_str()),
                    _ => None,
                })
                .collect(),
            fields
                .iter()
                .filter_map(|field| match field {
                    FieldConfig::Vector(config) => Some(config.field.as_str()),
                    _ => None,
                })
                .collect(),
        );

        Self {
            ids: Vec::with_capacity(size),
            chunks: Vec::with_capacity(size),
            metadata_fields: vec![Vec::with_capacity(size); metadata_names.len()],
            vector_fields: vec![Vec::with_capacity(size); vector_names.len()],
            field_mapping: FieldMapping {
                metadata_names,
                vector_names,
            },
        }
    }

    fn get_metadata_index(&self, field: &str) -> Option<usize> {
        self.field_mapping
            .metadata_names
            .iter()
            .position(|&name| name == field)
    }

    fn get_vector_index(&self, field: &str) -> Option<usize> {
        self.field_mapping
            .vector_names
            .iter()
            .position(|&name| name == field)
    }
}

impl PgVector {
    /// Generates a SQL statement to create a table for storing vector embeddings.
    ///
    /// The table will include columns for an ID, chunk data, metadata, and a vector embedding.
    ///
    /// # Returns
    ///
    /// * The generated SQL statement.
    ///
    /// # Errors
    ///
    /// * Returns an error if the table name is invalid or if `vector_size` is not configured.
    pub fn generate_create_table_sql(&self) -> Result<String> {
        // Validate table_name and field_name (e.g., check against allowed patterns)
        if !Self::is_valid_identifier(&self.table_name) {
            return Err(anyhow::anyhow!("Invalid table name"));
        }

        let columns: Vec<String> = self
            .fields
            .iter()
            .map(|field| match field {
                FieldConfig::ID => "id UUID NOT NULL".to_string(),
                FieldConfig::Chunk => format!("{} TEXT NOT NULL", field.field_name()),
                FieldConfig::Metadata(_) => format!("{} JSONB", field.field_name()),
                FieldConfig::Vector(_) => {
                    format!("{} VECTOR({})", field.field_name(), self.vector_size)
                }
            })
            .chain(std::iter::once("PRIMARY KEY (id)".to_string()))
            .collect();

        let sql = format!(
            "CREATE TABLE IF NOT EXISTS {} (\n  {}\n)",
            self.table_name,
            columns.join(",\n  ")
        );

        Ok(sql)
    }

    /// Generates the SQL statement to create an HNSW index on the vector column.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No vector field is found in the table configuration.
    /// - The table name or field name is invalid.
    pub fn create_index_sql(&self) -> Result<String> {
        let index_name = format!("{}_embedding_idx", self.table_name);
        let vector_field = self
            .fields
            .iter()
            .find(|f| matches!(f, FieldConfig::Vector(_)))
            .ok_or_else(|| anyhow::anyhow!("No vector field found in configuration"))?
            .field_name();

        // Validate table_name and field_name (e.g., check against allowed patterns)
        if !Self::is_valid_identifier(&self.table_name)
            || !Self::is_valid_identifier(&index_name)
            || !Self::is_valid_identifier(vector_field)
        {
            return Err(anyhow::anyhow!("Invalid table or field name"));
        }

        Ok(format!(
            "CREATE INDEX IF NOT EXISTS {} ON {} USING hnsw ({} vector_cosine_ops)",
            index_name, &self.table_name, vector_field
        ))
    }

    /// Stores a list of nodes in the database using an upsert operation.
    ///
    /// # Arguments
    ///
    /// * `nodes` - A slice of `Node` objects to be stored.
    ///
    /// # Returns
    ///
    /// * `Result<()>` - `Ok` if all nodes are successfully stored, `Err` otherwise.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The database connection pool is not established.
    /// - Any of the SQL queries fail to execute due to schema mismatch, constraint violations, or
    ///   connectivity issues.
    /// - Committing the transaction fails.
    pub async fn store_nodes(&self, nodes: &[Node]) -> Result<()> {
        let pool = self.pool_get_or_initialize().await?;

        let mut tx = pool.begin().await?;
        let bulk_data = self.prepare_bulk_data(nodes)?;

        let sql = self
            .sql_stmt_bulk_insert
            .get()
            .ok_or_else(|| anyhow!("SQL bulk insert statement not set"))?;

        let query = self.bind_bulk_data_to_query(sqlx::query(sql), &bulk_data)?;

        query
            .execute(&mut *tx)
            .await
            .map_err(|e| anyhow!("Failed to store nodes: {:?}", e))?;

        tx.commit()
            .await
            .map_err(|e| anyhow!("Failed to commit transaction: {:?}", e))
    }

    /// Prepares data from nodes into vectors for bulk processing.
    #[allow(clippy::implicit_clone)]
    fn prepare_bulk_data<'a>(&'a self, nodes: &'a [Node]) -> Result<BulkUpsertData<'a>> {
        let mut bulk_data = BulkUpsertData::new(&self.fields, nodes.len());

        for node in nodes {
            bulk_data.ids.push(node.id());
            bulk_data.chunks.push(node.chunk.as_str());

            for field in &self.fields {
                match field {
                    FieldConfig::Metadata(config) => {
                        let idx = bulk_data
                            .get_metadata_index(config.field.as_str())
                            .ok_or_else(|| anyhow!("Invalid metadata field"))?;

                        let value = node
                            .metadata
                            .get(&config.original_field)
                            .ok_or_else(|| anyhow!("Missing metadata field"))?;

                        let mut metadata_map = BTreeMap::new();
                        metadata_map.insert(config.original_field.clone(), value.clone());

                        bulk_data.metadata_fields[idx].push(serde_json::to_value(metadata_map)?);
                    }
                    FieldConfig::Vector(config) => {
                        let idx = bulk_data
                            .get_vector_index(config.field.as_str())
                            .ok_or_else(|| anyhow!("Invalid vector field"))?;

                        let data = node
                            .vectors
                            .as_ref()
                            .and_then(|v| v.get(&config.embedded_field))
                            .map(|v| v.to_vec())
                            .unwrap_or_default();

                        bulk_data.vector_fields[idx].push(ExtPgVector::Vector::from(data));
                    }
                    _ => (),
                }
            }
        }

        Ok(bulk_data)
    }

    /// Generates SQL for UNNEST-based bulk upsert.
    ///
    /// # Returns
    ///
    /// * `Result<String>` - The generated SQL statement or an error if fields are empty.
    ///
    /// # Errors
    ///
    /// Returns an error if `self.fields` is empty, as no valid SQL can be generated.
    pub(crate) fn generate_unnest_upsert_sql(&self) -> Result<String> {
        if self.fields.is_empty() {
            return Err(anyhow!("Cannot generate upsert SQL with empty fields"));
        }

        let mut columns = Vec::new();
        let mut unnest_params = Vec::new();
        let mut param_counter = 1;

        for field in &self.fields {
            let name = field.field_name();
            columns.push(name.to_string());

            unnest_params.push(format!(
                "${param_counter}::{}",
                match field {
                    FieldConfig::ID => "UUID[]",
                    FieldConfig::Chunk => "TEXT[]",
                    FieldConfig::Metadata(_) => "JSONB[]",
                    FieldConfig::Vector(_) => "VECTOR[]",
                }
            ));

            param_counter += 1;
        }

        let update_columns = self
            .fields
            .iter()
            .filter(|field| !matches!(field, FieldConfig::ID)) // Skip ID field in updates
            .map(|field| {
                let name = field.field_name();
                format!("{name} = EXCLUDED.{name}")
            })
            .collect::<Vec<_>>()
            .join(", ");

        Ok(format!(
            r"
            INSERT INTO {} ({})
            SELECT {}
            FROM UNNEST({}) AS t({})
            ON CONFLICT (id) DO UPDATE SET {}",
            self.table_name,
            columns.join(", "),
            columns.join(", "),
            unnest_params.join(", "),
            columns.join(", "),
            update_columns
        ))
    }

    /// Binds bulk data to the SQL query, ensuring data arrays are matched to corresponding fields.
    ///
    /// # Errors
    ///
    /// Returns an error if any metadata or vector field is missing from the bulk data.
    #[allow(clippy::implicit_clone)]
    fn bind_bulk_data_to_query<'a>(
        &self,
        mut query: sqlx::query::Query<'a, sqlx::Postgres, PgArguments>,
        bulk_data: &'a BulkUpsertData,
    ) -> Result<sqlx::query::Query<'a, sqlx::Postgres, PgArguments>> {
        for field in &self.fields {
            query = match field {
                FieldConfig::ID => query.bind(&bulk_data.ids),
                FieldConfig::Chunk => query.bind(&bulk_data.chunks),
                FieldConfig::Vector(config) => {
                    let idx = bulk_data
                        .get_vector_index(config.field.as_str())
                        .ok_or_else(|| {
                            anyhow!("Vector field {} not found in bulk data", config.field)
                        })?;
                    query.bind(&bulk_data.vector_fields[idx])
                }
                FieldConfig::Metadata(config) => {
                    let idx = bulk_data
                        .get_metadata_index(config.field.as_str())
                        .ok_or_else(|| {
                            anyhow!("Metadata field {} not found in bulk data", config.field)
                        })?;
                    query.bind(&bulk_data.metadata_fields[idx])
                }
            };
        }
        Ok(query)
    }

    /// Retrieves the name of the vector column configured in the schema.
    ///
    /// # Returns
    /// * `Ok(String)` - The name of the vector column if exactly one is configured.
    /// # Errors
    /// * `Error::NoEmbedding` - If no vector field is configured in the schema.
    /// * `Error::MultipleEmbeddings` - If multiple vector fields are configured in the schema.
    pub fn get_vector_column_name(&self) -> Result<String> {
        let vector_fields: Vec<_> = self
            .fields
            .iter()
            .filter(|field| matches!(field, FieldConfig::Vector(_)))
            .collect();

        match vector_fields.as_slice() {
            [field] => Ok(field.field_name().to_string()),
            [] => Err(anyhow!("No vector field configured in schema")),
            _ => Err(anyhow!(
                "Search strategy for multiple vector fields in the schema is not yet implemented"
            )),
        }
    }
}

impl PgVector {
    pub fn normalize_field_name(field: &str) -> String {
        // Define the special characters as an array
        let special_chars: [char; 4] = ['(', '[', '{', '<'];

        // First split by special characters and take the first part
        let base_text = field
            .split(|c| special_chars.contains(&c))
            .next()
            .unwrap_or(field)
            .trim();

        // Split by whitespace, take up to 3 words, convert to lowercase
        let normalized = base_text
            .split_whitespace()
            .take(3)
            .collect::<Vec<&str>>()
            .join("_")
            .to_lowercase();

        // Ensure the result only contains alphanumeric chars and underscores
        normalized
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .collect()
    }

    pub(crate) fn is_valid_identifier(identifier: &str) -> bool {
        // PostgreSQL identifier rules:
        // 1. Must start with a letter (a-z) or underscore
        // 2. Subsequent characters can be letters, underscores, digits (0-9), or dollar signs
        // 3. Maximum length is 63 bytes
        // 4. Cannot be a reserved keyword

        // Check length
        if identifier.is_empty() || identifier.len() > 63 {
            return false;
        }

        // Use a regular expression to check the pattern
        let identifier_regex = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_$]*$").unwrap();
        if !identifier_regex.is_match(identifier) {
            return false;
        }

        // Check if it's not a reserved keyword
        !Self::is_reserved_keyword(identifier)
    }

    pub(crate) fn is_reserved_keyword(word: &str) -> bool {
        // This list is not exhaustive. You may want to expand it based on
        // the PostgreSQL version you're using.
        const RESERVED_KEYWORDS: &[&str] = &[
            "SELECT", "FROM", "WHERE", "INSERT", "UPDATE", "DELETE", "DROP", "CREATE", "TABLE",
            "INDEX", "ALTER", "ADD", "COLUMN", "AND", "OR", "NOT", "NULL", "TRUE",
            "FALSE",
            // Add more keywords as needed
        ];

        RESERVED_KEYWORDS.contains(&word.to_uppercase().as_str())
    }
}

impl PgVector {
    async fn create_pool(&self) -> Result<PgPool> {
        let pool_options = PgPoolOptions::new().max_connections(self.db_max_connections);

        for attempt in 1..=self.db_max_retry {
            match pool_options.clone().connect(self.db_url.as_ref()).await {
                Ok(pool) => {
                    tracing::info!("Successfully established database connection");
                    return Ok(pool);
                }
                Err(err) if attempt < self.db_max_retry => {
                    tracing::warn!(
                        error = %err,
                        attempt = attempt,
                        max_retries = self.db_max_retry,
                        "Database connection attempt failed, retrying..."
                    );
                    sleep(self.db_conn_retry_delay).await;
                }
                Err(err) => {
                    return Err(anyhow!(err).context("Failed to establish database connection"));
                }
            }
        }

        Err(anyhow!(
            "Max connection retries ({}) exceeded",
            self.db_max_retry
        ))
    }

    /// Returns a reference to the `PgPool` if it is already initialized,
    /// or creates and initializes it if it is not.
    ///
    /// # Errors
    /// This function will return an error if pool creation fails.
    pub async fn pool_get_or_initialize(&self) -> Result<&PgPool> {
        if let Some(pool) = self.connection_pool.get() {
            return Ok(pool);
        }

        let pool = self.create_pool().await?;
        self.connection_pool
            .set(pool)
            .map_err(|_| anyhow!("Pool already initialized"))?;

        // Re-check if the pool was set successfully, otherwise return an error
        self.connection_pool
            .get()
            .ok_or_else(|| anyhow!("Failed to retrieve connection pool after setting it"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_identifiers() {
        assert!(PgVector::is_valid_identifier("valid_name"));
        assert!(PgVector::is_valid_identifier("_valid_name"));
        assert!(PgVector::is_valid_identifier("valid_name_123"));
        assert!(PgVector::is_valid_identifier("validName"));
    }

    #[test]
    fn test_invalid_identifiers() {
        assert!(!PgVector::is_valid_identifier("")); // Empty string
        assert!(!PgVector::is_valid_identifier(&"a".repeat(64))); // Too long
        assert!(!PgVector::is_valid_identifier("123_invalid")); // Starts with a number
        assert!(!PgVector::is_valid_identifier("invalid-name")); // Contains hyphen
        assert!(!PgVector::is_valid_identifier("select")); // Reserved keyword
    }
}
