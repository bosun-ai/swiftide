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
    Persist,
};
use uuid::Uuid;

use super::Duckdb;

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
            // Should be borrow-able
            NodeValues::Path(path) => Ok(path.to_string_lossy().to_string().into()),
            NodeValues::Chunk(chunk) => chunk.to_sql(),
            NodeValues::Metadata(_metadata) => {
                unimplemented!("maps are not yet implemented for duckdb");
                // Casting doesn't work either, the duckdb conversion is also not implemented :(
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
            .unwrap()
            .execute_batch(&self.schema)
            .context("Failed to create indexing table")?;

        tracing::info!("Setup completed");

        Ok(())
    }

    async fn store(&self, node: indexing::Node) -> Result<indexing::Node> {
        let lock = self.connection.lock().unwrap();
        let mut stmt = lock.prepare(&self.node_upsert_sql)?;
        self.store_node_on_stmt(&mut stmt, &node)?;

        Ok(node)
    }

    async fn batch_store(&self, nodes: Vec<indexing::Node>) -> indexing::IndexingStream {
        // TODO: Must batch
        let mut new_nodes = Vec::with_capacity(nodes.len());

        tracing::debug!("Waiting for transaction");
        let mut conn = self.connection.lock().unwrap();
        tracing::debug!("Got transaction");
        let tx = match conn.transaction().context("Failed to start transaction") {
            Ok(tx) => tx,
            Err(err) => {
                return Err(err).into();
            }
        };

        tracing::debug!("Starting batch store");
        {
            let mut stmt = match tx
                .prepare(&self.node_upsert_sql)
                .context("Failed to prepare statement")
            {
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
    use futures_util::TryStreamExt as _;
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

        {
            let connection = client.connection.lock().unwrap();
            let mut stmt = connection
                .prepare("SELECT uuid,path,chunk FROM test")
                .unwrap();
            let node_iter = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0).unwrap(), // id
                        row.get::<_, String>(1).unwrap(), // chunk
                        row.get::<_, String>(2).unwrap(), // path
                    ))
                })
                .unwrap();

            let retrieved = node_iter.collect::<Result<Vec<_>, _>>().unwrap();
            //
            assert_eq!(retrieved.len(), 1);
        }

        tracing::info!("Retrieved node");
        // Verify the upsert and batch works
        let new_nodes = vec![node.clone(), node.clone(), node.clone()];
        let stream_nodes: Vec<Node> = client
            .batch_store(new_nodes)
            .await
            .try_collect()
            .await
            .unwrap();

        // let streamed_nodes: Vec<Node> = stream.try_collect().await.unwrap();
        assert_eq!(stream_nodes.len(), 3);
        assert_eq!(stream_nodes[0], node);

        tracing::info!("Batch stored nodes 1");
        {
            let connection = client.connection.lock().unwrap();
            let mut stmt = connection
                .prepare("SELECT uuid,path,chunk FROM test")
                .unwrap();
            let node_iter = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0).unwrap(), // id
                        row.get::<_, String>(1).unwrap(), // chunk
                        row.get::<_, String>(2).unwrap(), // path
                    ))
                })
                .unwrap();

            let retrieved = node_iter.collect::<Result<Vec<_>, _>>().unwrap();
            assert_eq!(retrieved.len(), 1);
        }

        // Test batch store fully
        let mut new_node = node.clone();
        new_node.chunk = "Something else".into();

        let new_nodes = vec![node.clone(), new_node.clone(), new_node.clone()];
        let stream = client.batch_store(new_nodes).await;

        let streamed_nodes: Vec<Node> = stream.try_collect().await.unwrap();
        assert_eq!(streamed_nodes.len(), 3);
        assert_eq!(streamed_nodes[0], node);

        {
            let connection = client.connection.lock().unwrap();
            let mut stmt = connection
                .prepare("SELECT uuid,path,chunk FROM test")
                .unwrap();

            let node_iter = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0).unwrap(), // id
                        row.get::<_, String>(1).unwrap(), // chunk
                        row.get::<_, String>(2).unwrap(), // path
                    ))
                })
                .unwrap();
            let retrieved = node_iter.collect::<Result<Vec<_>, _>>().unwrap();
            assert_eq!(retrieved.len(), 2);
        }
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

        let connection = client.connection.lock().unwrap();
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
