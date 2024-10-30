//! This module provides functionality to convert a `Node` into a `PostgreSQL` table schema.
//! This conversion is crucial for storing data in `PostgreSQL`, enabling efficient vector similarity searches
//! through the `pgvector` extension. The module also handles metadata augmentation and ensures compatibility
//! with `PostgreSQL`'s required data format.

use crate::pgvector::PgVector;
use anyhow::{anyhow, Context, Result};
use pgvector as ExtPgVector;
use regex::Regex;
use sqlx::postgres::PgArguments;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::BTreeMap;
use std::sync::Arc;
use swiftide_core::indexing::{EmbeddedField, Node};
use tokio::time::{sleep, Duration};

#[derive(Clone)]
pub struct PgDBConnectionPool(Arc<Option<PgPool>>);

impl Default for PgDBConnectionPool {
    fn default() -> Self {
        Self(Arc::new(None))
    }
}

impl PgDBConnectionPool {
    /// Attempts to connect to the database with retries.
    async fn connect_with_retry(
        database_url: impl AsRef<str>,
        max_retries: u32,
        pool_options: &PgPoolOptions,
    ) -> Result<PgPool, sqlx::Error> {
        for attempt in 1..=max_retries {
            match pool_options.clone().connect(database_url.as_ref()).await {
                Ok(pool) => {
                    return Ok(pool);
                }
                Err(_err) if attempt < max_retries => {
                    sleep(Duration::from_secs(2)).await;
                }
                Err(err) => return Err(err),
            }
        }
        unreachable!()
    }

    /// Connects to the database using the provided URL and sets the connection pool.
    pub async fn try_connect_to_url(
        mut self,
        database_url: impl AsRef<str>,
        connection_max: Option<u32>,
    ) -> Result<Self> {
        let pool_options = PgPoolOptions::new().max_connections(connection_max.unwrap_or(10));

        let pool = Self::connect_with_retry(database_url, 10, &pool_options)
            .await
            .context("Failed to connect to the database")?;

        self.0 = Arc::new(Some(pool));

        Ok(self)
    }

    /// Retrieves the connection pool, returning an error if the pool is not initialized.
    pub fn get_pool(&self) -> Result<PgPool> {
        self.0
            .as_ref()
            .clone()
            .ok_or_else(|| anyhow!("Database connection pool is not initialized"))
    }

    /// Returns the connection status of the pool.
    pub fn connection_status(&self) -> &'static str {
        match self.0.as_ref() {
            Some(pool) if !pool.is_closed() => "Open",
            Some(_) => "Closed",
            None => "Not initialized",
        }
    }
}

#[derive(Clone, Debug)]
pub struct VectorConfig {
    embedded_field: EmbeddedField,
    field: String,
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

#[derive(Clone, Debug)]
pub enum FieldConfig {
    Vector(VectorConfig),
    Metadata(MetadataConfig),
    Chunk,
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

/// Structure to hold collected values for bulk upsert
#[derive(Default)]
struct BulkUpsertData {
    ids: Vec<sqlx::types::Uuid>,
    chunks: Vec<String>,
    metadata_fields: BTreeMap<String, Vec<serde_json::Value>>,
    vector_fields: BTreeMap<String, Vec<ExtPgVector::Vector>>,
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
    /// *  Returns an error if the table name is invalid or if `vector_size` is not configured.
    pub fn generate_create_table_sql(&self) -> Result<String> {
        // Validate table_name and field_name (e.g., check against allowed patterns)
        if !Self::is_valid_identifier(&self.table_name) {
            return Err(anyhow::anyhow!("Invalid table name"));
        }

        let vector_size = self
            .vector_size
            .ok_or_else(|| anyhow!("vector_size must be configured"))?;

        let columns: Vec<String> = self
            .fields
            .iter()
            .map(|field| match field {
                FieldConfig::ID => "id UUID NOT NULL".to_string(),
                FieldConfig::Chunk => format!("{} TEXT NOT NULL", field.field_name()),
                FieldConfig::Metadata(_) => format!("{} JSONB", field.field_name()),
                FieldConfig::Vector(_) => format!("{} VECTOR({})", field.field_name(), vector_size),
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
    /// - Any of the SQL queries fail to execute due to schema mismatch, constraint violations, or connectivity issues.
    /// - Committing the transaction fails.
    pub async fn store_nodes(&self, nodes: &[Node]) -> Result<()> {
        let pool = self.connection_pool.get_pool()?;

        let mut tx = pool.begin().await?;
        let bulk_data = self.prepare_bulk_data(nodes)?;
        let sql = self.generate_unnest_upsert_sql()?;

        let query = self.bind_bulk_data_to_query(sqlx::query(&sql), &bulk_data)?;

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
    fn prepare_bulk_data(&self, nodes: &[Node]) -> Result<BulkUpsertData> {
        let mut bulk_data = BulkUpsertData::default();

        for node in nodes {
            bulk_data.ids.push(node.id());
            bulk_data.chunks.push(node.chunk.clone());

            for field in &self.fields {
                match field {
                    FieldConfig::Metadata(config) => {
                        let value = node.metadata.get(&config.original_field).ok_or_else(|| {
                            anyhow!("Metadata field {} not found", config.original_field)
                        })?;

                        let entry = bulk_data
                            .metadata_fields
                            .entry(config.field.clone())
                            .or_default();

                        let mut metadata_map = BTreeMap::new();
                        metadata_map.insert(config.original_field.clone(), value.clone());
                        entry.push(serde_json::to_value(metadata_map)?);
                    }
                    FieldConfig::Vector(config) => {
                        let data = node
                            .vectors
                            .as_ref()
                            .and_then(|v| v.get(&config.embedded_field))
                            .map(|v| v.to_vec())
                            .unwrap_or_default();

                        bulk_data
                            .vector_fields
                            .entry(config.field.clone())
                            .or_default()
                            .push(ExtPgVector::Vector::from(data));
                    }
                    _ => continue, // ID and Chunk already handled
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
    fn generate_unnest_upsert_sql(&self) -> Result<String> {
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
            r#"
            INSERT INTO {} ({})
            SELECT {}
            FROM UNNEST({}) AS t({})
            ON CONFLICT (id) DO UPDATE SET {}"#,
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
                FieldConfig::Metadata(config) => {
                    let values = bulk_data
                        .metadata_fields
                        .get(&config.field)
                        .ok_or_else(|| {
                            anyhow!("Metadata field {} not found in bulk data", config.field)
                        })?;
                    query.bind(values)
                }
                FieldConfig::Vector(config) => {
                    let vectors = bulk_data.vector_fields.get(&config.field).ok_or_else(|| {
                        anyhow!("Vector field {} not found in bulk data", config.field)
                    })?;
                    query.bind(vectors)
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
            _ => Err(anyhow!("Multiple vector fields configured in schema")),
        }
    }
}

impl PgVector {
    pub(crate) fn normalize_field_name(field: &str) -> String {
        field
            .to_lowercase()
            .replace(|c: char| !c.is_alphanumeric(), "_")
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
