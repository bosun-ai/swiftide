use std::{collections::HashMap, path::Path};

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use duckdb::{
    params_from_iter,
    types::{OrderedMap, ToSqlOutput, Value},
    ToSql,
};
use swiftide_core::{
    indexing::{self, EmbeddedField, Metadata},
    template::{Context, Template},
    Persist,
};
use uuid::Uuid;

use super::Duckdb;

const SCHEMA: &str = include_str!("schema.sql");
const UPSERT: &str = include_str!("upsert.sql");

enum NodeValues<'a> {
    Uuid(Uuid),
    Path(&'a Path),
    Chunk(&'a str),
    Metadata(&'a Metadata),
    Vector(&'a [f32]),
}

impl ToSql for NodeValues<'_> {
    fn to_sql(&self) -> duckdb::Result<ToSqlOutput<'_>> {
        match self {
            NodeValues::Uuid(uuid) => Ok(ToSqlOutput::Owned(uuid.to_string().into())),
            NodeValues::Path(path) => Ok(path.to_string_lossy().to_string().into()), // Should be borrow-able
            NodeValues::Chunk(chunk) => chunk.to_sql(),
            NodeValues::Metadata(metadata) => {
                let ordered_map: OrderedMap<Value, Value> = metadata
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.to_string().into(),
                            serde_json::to_string(v).unwrap().into(),
                        )
                    })
                    .collect::<Vec<(_, _)>>()
                    .into();
                Ok(ToSqlOutput::Owned(duckdb::types::Value::Map(ordered_map)))
            }
            NodeValues::Vector(vector) => Ok(ToSqlOutput::Owned(Value::Array(
                vector.iter().map(|f| (*f).into()).collect(),
            ))),
        }
    }
}

#[async_trait]
impl Persist for Duckdb {
    async fn setup(&self) -> Result<()> {
        let mut context = Context::default();
        context.insert("table_name", &self.table_name);
        context.insert("vectors", &self.vectors);

        let rendered = Template::Static(SCHEMA).render(&context).await?;
        self.connection.lock().await.execute_batch(&rendered)?;

        context.insert(
            "vector_field_names",
            &self.vectors.keys().collect::<Vec<_>>(),
        );

        // User could have overridden the upsert sql
        // Which is fine
        let upsert = Template::Static(UPSERT).render(&context).await?;
        self.node_upsert_sql
            .set(upsert)
            .map_err(|_| anyhow::anyhow!("Failed to set upsert sql"))?;

        Ok(())
    }

    async fn store(&self, node: indexing::Node) -> Result<indexing::Node> {
        let Some(query) = self.node_upsert_sql.get() else {
            anyhow::bail!("Upsert sql in Duckdb not set");
        };

        // TODO: Doing potentially many locks here for the duration of a single query,
        // SOMEONE IS GOING TO HAVE A BAD TIME
        let lock = self.connection.lock().await;
        let mut stmt = lock.prepare(query)?;

        // metadata needs to be converted to `map_from_entries([('key1', value)])``
        // TODO: Investigate if we can do with way less allocations
        let mut values = vec![
            NodeValues::Uuid(node.id()),
            NodeValues::Chunk(&node.chunk),
            NodeValues::Path(&node.path),
            NodeValues::Metadata(&node.metadata),
        ];

        let Some(node_vectors) = &node.vectors else {
            anyhow::bail!("Expected node to have vectors; cannot store into duckdb");
        };

        for (field, size) in &self.vectors {
            let Some(vector) = node_vectors.get(field) else {
                anyhow::bail!("Expected vector for field {} in node", field);
            };

            values.push(NodeValues::Vector(vector));
        }

        // TODO: Investigate concurrency in duckdb, maybe optmistic if it works
        stmt.execute(params_from_iter(values))
            .context("Failed to store node")?;

        Ok(node)
    }

    async fn batch_store(&self, nodes: Vec<indexing::Node>) -> indexing::IndexingStream {
        // TODO: Must batch
        let mut new_nodes = vec![];
        for node in nodes {
            new_nodes.push(self.store(node).await);
        }
        new_nodes.into()
    }
}

#[cfg(test)]
mod tests {
    use indexing::{EmbeddedField, Node};

    use super::*;

    #[test_log::test(tokio::test)]
    async fn test_persisting_nodes() {
        let client = Duckdb::builder()
            .connection(duckdb::Connection::open_in_memory().unwrap())
            .table_name("test".to_string())
            .with_vector(EmbeddedField::Combined, 3)
            .build()
            .unwrap();

        let node = Node::new("Hello duckdb!")
            .with_vectors([(EmbeddedField::Combined, vec![1.0, 2.0, 3.0])])
            .to_owned();

        client.setup().await.unwrap();
        client.store(node).await.unwrap();

        let connection = client.connection.lock().await;
        let mut stmt = connection.prepare("SELECT * FROM test").unwrap();
        let node_iter = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0).unwrap(), // id
                    row.get::<_, String>(1).unwrap(), // chunk
                    row.get::<_, String>(2).unwrap(), // path
                    row.get::<_, String>(3).unwrap(), // metadata
                    row.get::<_, String>(4).unwrap(), // vector
                ))
            })
            .unwrap();

        let retrieved = node_iter.collect::<Result<Vec<_>, _>>().unwrap();

        assert_eq!(retrieved.len(), 1);
        assert_eq!(retrieved[0].1, "Hello duckdb!");
    }
}
