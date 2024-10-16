//! This module provides functionality to convert a `Node` into a `PostgreSQL` table schema.
//! This conversion is crucial for storing data in `PostgreSQL`, enabling efficient vector similarity searches
//! through the `pgvector` extension. The module also handles metadata augmentation and ensures compatibility
//! with PostgreSQL's required data format.

use crate::pgvector::PgVector;
use anyhow::{anyhow, Result};
use pgvector as ExtPgVector;
use swiftide_core::indexing::{EmbeddedField, Node};

pub(crate) fn normalize_field_name(field: &str) -> String {
    field
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric(), "_")
}

#[derive(Clone, Debug)]
pub struct VectorConfig {
    embedded_field: EmbeddedField,
}

impl VectorConfig {
    pub fn new(embedded_field: EmbeddedField) -> Self {
        Self { embedded_field }
    }

    pub fn field_name(&self) -> String {
        format!(
            "vector_{}",
            normalize_field_name(&self.embedded_field.to_string()),
        )
    }
}

impl From<EmbeddedField> for VectorConfig {
    fn from(val: EmbeddedField) -> Self {
        Self::new(val)
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
            field: "qa_metadata".to_string(),
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
    pub fn field_name(&self) -> String {
        match self {
            FieldConfig::Vector(config) => config.field_name(),
            FieldConfig::Metadata(config) => config.field.clone(),
            FieldConfig::Chunk => "chunk".into(),
            FieldConfig::ID => "id".into(),
        }
    }
}

impl PgVector {
    /// Creates an SQL statement to create a table with the specified fields.
    ///
    /// **Arguments:**
    ///   - `self`: A reference to the `vec<FieldConfig>` field, which contains the
    ///     configuration for each field in the table.
    ///
    /// **Returns:**
    ///   - `Result<String, _>`:
    ///     - **Success:** The generated SQL statement as a string.
    ///
    /// # Errors
    ///
    /// This function will return an error if any part of SQL generation fails
    /// or if the field configuration is incorrect.
    pub fn generate_create_table_sql(&self) -> Result<String> {
        let columns: Vec<String> = self
            .fields
            .iter()
            .map(|field| match field {
                FieldConfig::ID => "id UUID PRIMARY KEY".to_string(),
                FieldConfig::Chunk => format!("{} TEXT NOT NULL", field.field_name()),
                FieldConfig::Metadata(_) => format!("{} JSONB", field.field_name()),
                FieldConfig::Vector(_) => {
                    format!("{} VECTOR({})", field.field_name(), self.vector_size)
                }
            })
            .collect();

        let sql = format!(
            "CREATE TABLE IF NOT EXISTS {} (\n  {}\n)",
            self.table_name,
            columns.join(",\n  ")
        );

        Ok(sql)
    }

    /// Stores a list of nodes in the database using an upsert operation.
    ///
    /// # Arguments
    ///
    /// * `nodes` - A slice of `Node` objects to be stored.
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Ok if all nodes are successfully stored, Err otherwise.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The database connection pool is not established.
    /// - Any of the SQL queries fail to execute.
    /// - Committing the transaction fails.
    pub async fn store_nodes(&self, nodes: &[Node]) -> Result<()> {
        let pool = self
            .connection_pool
            .as_ref()
            .ok_or_else(|| anyhow!("Database connection not established"))?;

        let mut tx = pool.begin().await?;
        let sql = self.generate_upsert_sql();

        for (i, node) in nodes.iter().enumerate() {
            tracing::debug!("Storing node {}: {:?}", i, node.id());

            let query = self.bind_node_to_query(&sql, node)?;

            query.execute(&mut *tx).await.map_err(|e| {
                tracing::error!("Failed to store node {}: {:?}", i, e);
                anyhow!("Failed to store node {}: {:?}", i, e)
            })?;

            tracing::debug!("Successfully stored node {}", i);
        }

        tx.commit()
            .await
            .map_err(|e| anyhow!("Failed to commit transaction: {:?}", e))?;

        Ok(())
    }

    /// Generates an SQL upsert statement based on the current fields and table name.
    ///
    /// This function constructs a SQL query that inserts new rows into a `PostgreSQL` database table
    /// if they do not already exist (based on the "id" column), or updates them if they do. The generated
    /// SQL is intended to be efficient and safe for concurrent use.
    #[allow(clippy::redundant_closure_for_method_calls)]
    fn generate_upsert_sql(&self) -> String {
        let columns: Vec<_> = self.fields.iter().map(|f| f.field_name()).collect();
        let placeholders: Vec<_> = (1..=self.fields.len()).map(|i| format!("${i}")).collect();
        // Iterate over each field in the fields vector, keeping track of the index
        let updates: Vec<_> = self
            .fields
            .iter()
            .enumerate()
            .filter(|(_, f)| f.field_name() != "id")
            .map(|(i, f)| format!("{} = ${}", f.field_name(), i + 1))
            .collect();

        // Construct the final SQL upsert statement using the collected columns, placeholders, and updates
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT (id) DO UPDATE SET {}",
            self.table_name,
            columns.join(", "),
            placeholders.join(", "),
            updates.join(", ")
        );

        tracing::debug!("Generated SQL: {}", sql);
        sql
    }

    #[allow(clippy::implicit_clone)]
    fn bind_node_to_query<'a>(
        &self,
        sql: &'a str,
        node: &'a Node,
    ) -> Result<sqlx::query::Query<'a, sqlx::Postgres, sqlx::postgres::PgArguments>> {
        let mut query = sqlx::query(sql);

        for field in &self.fields {
            query = match field {
                FieldConfig::ID => query.bind(node.id()),
                FieldConfig::Chunk => query.bind(&node.chunk),
                FieldConfig::Metadata(config) => {
                    let value = node.metadata.get(&config.original_field).ok_or_else(|| {
                        anyhow!("Metadata field {} not found", config.original_field)
                    })?;
                    query.bind(serde_json::to_value(value)?)
                }
                FieldConfig::Vector(config) => {
                    let data = node
                        .vectors
                        .as_ref()
                        .and_then(|v| v.get(&config.embedded_field))
                        .map(|v| v.to_vec())
                        .unwrap_or_default();
                    query.bind(ExtPgVector::Vector::from(data))
                }
            };
        }

        Ok(query)
    }
}
