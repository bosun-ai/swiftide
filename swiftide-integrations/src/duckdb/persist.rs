use std::{borrow::Cow, path::Path};

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use duckdb::{
    params_from_iter,
    types::{ToSqlOutput, Value},
    Statement, ToSql,
};
use swiftide_core::{
    indexing::{self, Metadata, Node},
    template::{Context, Template},
    Persist,
};
use uuid::Uuid;

use super::Duckdb;

const UPSERT: &str = include_str!("upsert.sql");

#[allow(dead_code)]
enum NodeValues<'a> {
    Uuid(Uuid),
    Path(&'a Path),
    Chunk(&'a str),
    Metadata(&'a Metadata),
    Embedding(Cow<'a, [f32]>),
    Null,
}

impl ToSql for NodeValues<'_> {
    fn to_sql(&self) -> duckdb::Result<ToSqlOutput<'_>> {
        match self {
            NodeValues::Uuid(uuid) => Ok(ToSqlOutput::Owned(uuid.to_string().into())),
            NodeValues::Path(path) => Ok(path.to_string_lossy().to_string().into()), // Should be borrow-able
            NodeValues::Chunk(chunk) => chunk.to_sql(),
            NodeValues::Metadata(_metadata) => {
                unimplemented!("maps are not yet implemented for duckdb");
                // Casting doesn't work either, the duckdb conversion is also not implemented :(
                // if metadata.is_empty() {
                //     return Ok(ToSqlOutput::Owned(Value::Null));
                // }
                // // let ordered_map = metadata
                // //     .iter()
                // //     .map(|(k, v)| format!("'{}': '{}'", k, serde_json::to_string(v).unwrap()))
                // //     .collect::<Vec<_>>()
                // //     .join(",");
                // let ordered_map = metadata
                //     .iter()
                //     .map(|(k, v)| format!("('{}', '{}')", k, serde_json::to_string(v).unwrap()))
                //     .collect::<Vec<_>>()
                //     .join(",");
                //
                // let formatted = format!("map_from_entries([{ordered_map}])");
                // dbg!(&formatted);
                // Ok(ToSqlOutput::Owned(formatted.into()))
                // Ok(ToSqlOutput::Owned(formatted.into()))
            }
            NodeValues::Embedding(vector) => {
                let array_str = format!(
                    "[{}]",
                    vector
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                );
                Ok(ToSqlOutput::Owned(array_str.into()))
            }
            NodeValues::Null => Ok(ToSqlOutput::Owned(Value::Null)),
        }
    }
}

impl Duckdb {
    fn store_node_on_stmt(&self, stmt: &mut Statement<'_>, node: &Node) -> Result<()> {
        let mut values = vec![
            NodeValues::Uuid(node.id()),
            NodeValues::Chunk(&node.chunk),
            NodeValues::Path(&node.path),
        ];

        let Some(node_vectors) = &node.vectors else {
            anyhow::bail!("Expected node to have vectors; cannot store into duckdb");
        };

        for field in self.vectors.keys() {
            let Some(vector) = node_vectors.get(field) else {
                anyhow::bail!("Expected vector for field {} in node", field);
            };

            values.push(NodeValues::Embedding(vector.into()));
        }

        // TODO: Investigate concurrency in duckdb, maybe optmistic if it works
        stmt.execute(params_from_iter(values))
            .context("Failed to store node")?;

        Ok(())
    }
}

#[async_trait]
impl Persist for Duckdb {
    async fn setup(&self) -> Result<()> {
        self.connection
            .lock()
            .await
            .execute_batch(&self.schema)
            .context("Failed to create indexing table")?;

        let mut context = Context::default();
        context.insert("table_name", &self.table_name);
        context.insert("vectors", &self.vectors);

        context.insert(
            "vector_field_names",
            &self.vectors.keys().collect::<Vec<_>>(),
        );

        tracing::info!("Rendering upsert sql");
        let upsert = Template::Static(UPSERT).render(&context).await?;
        self.node_upsert_sql
            .set(upsert)
            .map_err(|_| anyhow::anyhow!("Failed to set upsert sql"))?;

        tracing::info!("Setup completed");

        Ok(())
    }

    async fn store(&self, node: indexing::Node) -> Result<indexing::Node> {
        let Some(query) = self.node_upsert_sql.get() else {
            anyhow::bail!("Upsert sql in Duckdb not set");
        };

        let lock = self.connection.lock().await;
        let mut stmt = lock.prepare(query)?;
        self.store_node_on_stmt(&mut stmt, &node)?;

        Ok(node)
    }

    async fn batch_store(&self, nodes: Vec<indexing::Node>) -> indexing::IndexingStream {
        // TODO: Must batch
        let mut new_nodes = Vec::with_capacity(nodes.len());

        let Some(query) = self.node_upsert_sql.get() else {
            return Err(anyhow::anyhow!("Upsert sql in Duckdb not set")).into();
        };

        let mut conn = self.connection.lock().await;
        let tx = match conn.transaction().context("Failed to start transaction") {
            Ok(tx) => tx,
            Err(err) => {
                return Err(err).into();
            }
        };

        {
            let mut stmt = match tx.prepare(query).context("Failed to prepare statement") {
                Ok(stmt) => stmt,
                Err(err) => {
                    return Err(err).into();
                }
            };

            for node in nodes {
                new_nodes.push(self.store_node_on_stmt(&mut stmt, &node).map(|()| node));
            }
        };
        if let Err(err) = tx.commit().context("Failed to commit transaction") {
            return Err(err).into();
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
        client.store(node.clone()).await.unwrap();

        tracing::info!("Stored node");

        let connection = client.connection.lock().await;
        let mut stmt = connection
            .prepare("SELECT uuid,path,chunk FROM test")
            .unwrap();
        let node_iter = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0).unwrap(), // id
                    row.get::<_, String>(1).unwrap(), // chunk
                    row.get::<_, String>(2).unwrap(), // path
                                                      // row.get::<_, String>(3).unwrap(), // metadata
                                                      // row.get::<_, Vec<f32>>(4).unwrap(), // vector
                ))
            })
            .unwrap();

        let retrieved = node_iter.collect::<Result<Vec<_>, _>>().unwrap();
        //
        assert_eq!(retrieved.len(), 1);

        // Verify the upsert and batch works
        let _ = client.batch_store(vec![node.clone(), node.clone(), node.clone()]);
        let node_iter = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0).unwrap(), // id
                    row.get::<_, String>(1).unwrap(), // chunk
                    row.get::<_, String>(2).unwrap(), // path
                                                      // row.get::<_, String>(3).unwrap(), // metadata
                                                      // row.get::<_, Vec<f32>>(4).unwrap(), // vector
                ))
            })
            .unwrap();

        let retrieved = node_iter.collect::<Result<Vec<_>, _>>().unwrap();
        //
        assert_eq!(retrieved.len(), 1);
    }

    #[ignore]
    #[test_log::test(tokio::test)]
    async fn test_with_metadata() {
        let client = Duckdb::builder()
            .connection(duckdb::Connection::open_in_memory().unwrap())
            .table_name("test".to_string())
            .with_vector(EmbeddedField::Combined, 3)
            .build()
            .unwrap();

        let mut node = Node::new("Hello duckdb!")
            .with_vectors([(EmbeddedField::Combined, vec![1.0, 2.0, 3.0])])
            .to_owned();

        node.metadata
            .insert("filter".to_string(), "true".to_string());

        client.setup().await.unwrap();
        client.store(node).await.unwrap();

        tracing::info!("Stored node");

        let connection = client.connection.lock().await;
        let mut stmt = connection
            .prepare("SELECT uuid,path,chunk FROM test")
            .unwrap();

        let node_iter = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0).unwrap(), // id
                    row.get::<_, String>(1).unwrap(), // chunk
                    row.get::<_, String>(2).unwrap(), // path
                    row.get::<_, Value>(3).unwrap(),  // path
                                                      // row.get::<_, String>(3).unwrap(), // metadata
                                                      // row.get::<_, Vec<f32>>(4).unwrap(), // vector
                ))
            })
            .unwrap();

        let retrieved = node_iter.collect::<Result<Vec<_>, _>>().unwrap();
        dbg!(&retrieved);
        //
        assert_eq!(retrieved.len(), 1);

        let Value::Map(metadata) = &retrieved[0].3 else {
            panic!("Expected metadata to be a map");
        };

        assert_eq!(metadata.keys().count(), 1);
        assert_eq!(
            metadata.get(&Value::Text("filter".into())).unwrap(),
            &Value::Text("true".into())
        );
    }
}
