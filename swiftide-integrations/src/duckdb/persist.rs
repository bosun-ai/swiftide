use std::{borrow::Cow, path::Path};

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use duckdb::{
    Statement, ToSql, params, params_from_iter,
    types::{ToSqlOutput, Value},
};
use swiftide_core::{
    Persist,
    indexing::{self, Chunk, Metadata, Node},
};
use uuid::Uuid;

use super::Duckdb;

#[allow(dead_code)]
enum TextNodeValues<'a> {
    Uuid(Uuid),
    Path(&'a Path),
    Chunk(&'a str),
    Metadata(&'a Metadata),
    Embedding(Cow<'a, [f32]>),
    Null,
}

impl ToSql for TextNodeValues<'_> {
    fn to_sql(&self) -> duckdb::Result<ToSqlOutput<'_>> {
        match self {
            TextNodeValues::Uuid(uuid) => Ok(ToSqlOutput::Owned(uuid.to_string().into())),
            // Should be borrow-able
            TextNodeValues::Path(path) => Ok(path.to_string_lossy().to_string().into()),
            TextNodeValues::Chunk(chunk) => chunk.to_sql(),
            TextNodeValues::Metadata(_metadata) => {
                unimplemented!("maps are not yet implemented for duckdb");
                // Casting doesn't work either, the duckdb conversion is also not implemented :(
            }
            TextNodeValues::Embedding(vector) => {
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
            TextNodeValues::Null => Ok(ToSqlOutput::Owned(Value::Null)),
        }
    }
}

impl<T: Chunk + AsRef<str>> Duckdb<T> {
    fn store_node_on_stmt(&self, stmt: &mut Statement<'_>, node: &Node<T>) -> Result<()> {
        let mut values = vec![
            TextNodeValues::Uuid(node.id()),
            TextNodeValues::Chunk(node.chunk.as_ref()),
            TextNodeValues::Path(&node.path),
        ];

        let Some(node_vectors) = &node.vectors else {
            anyhow::bail!("Expected node to have vectors; cannot store into duckdb");
        };

        for field in self.vectors.keys() {
            let Some(vector) = node_vectors.get(field) else {
                anyhow::bail!("Expected vector for field {field} in node");
            };

            values.push(TextNodeValues::Embedding(vector.into()));
        }

        // TODO: Investigate concurrency in duckdb, maybe optmistic if it works
        stmt.execute(params_from_iter(values))
            .context("Failed to store node")?;

        Ok(())
    }
}

#[async_trait]
impl<T: Chunk + AsRef<str>> Persist for Duckdb<T> {
    type Input = T;
    type Output = T;

    async fn setup(&self) -> Result<()> {
        tracing::debug!("Setting up duckdb schema");

        {
            let conn = self.connection.lock().unwrap();

            // Create if not exists does not seem to work with duckdb, so we check first
            if conn
                // Duckdb has issues with params it seems.
                .query_row(&format!("SHOW {}", self.table_name()), params![], |row| {
                    row.get::<_, String>(0)
                })
                .is_ok()
            {
                tracing::debug!("Indexing table already exists, skipping creation");
                return Ok(());
            }

            // Install the extensions separately from the schema to avoid duckdb issues with random
            // 'extension exists' errors
            let _ = conn.execute_batch(include_str!("extensions.sql"));

            conn.execute_batch(&self.schema)
                .context("Failed to create indexing table")?;

            tracing::debug!(schema = &self.schema, "Indexing table created");
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        {
            let conn = self.connection.lock().unwrap();
            // We need to run this separately to ensure the table is created before we create the
            // index
            conn.execute_batch(&format!(
                "PRAGMA create_fts_index('{}', 'uuid', 'chunk', stemmer = 'porter',
                 stopwords = 'english', ignore = '(\\.|[^a-z])+',
                 strip_accents = 1, lower = 1, overwrite = 0);
",
                self.table_name
            ))?;
        }

        tracing::info!("Setup completed");

        Ok(())
    }

    async fn store(&self, node: indexing::Node<T>) -> Result<indexing::Node<T>> {
        let lock = self.connection.lock().unwrap();
        let mut stmt = lock.prepare(&self.node_upsert_sql)?;
        self.store_node_on_stmt(&mut stmt, &node)?;

        Ok(node)
    }

    async fn batch_store(&self, nodes: Vec<indexing::Node<T>>) -> indexing::IndexingStream<T> {
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
    use indexing::{EmbeddedField, TextNode};

    use super::*;

    #[test_log::test(tokio::test)]
    async fn test_persisting_nodes() {
        let client = Duckdb::builder()
            .connection(duckdb::Connection::open_in_memory().unwrap())
            .table_name("test".to_string())
            .with_vector(EmbeddedField::Combined, 3)
            .build()
            .unwrap();

        let node = TextNode::new("Hello duckdb!")
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
        let stream_nodes: Vec<TextNode> = client
            .batch_store(new_nodes)
            .await
            .try_collect()
            .await
            .unwrap();

        // let streamed_nodes: Vec<TextNode> = stream.try_collect().await.unwrap();
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

        let streamed_nodes: Vec<TextNode> = stream.try_collect().await.unwrap();
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

    #[ignore = "json types are acting up in duckdb at the moment"]
    #[test_log::test(tokio::test)]
    async fn test_with_metadata() {
        let client = Duckdb::builder()
            .connection(duckdb::Connection::open_in_memory().unwrap())
            .table_name("test".to_string())
            .with_vector(EmbeddedField::Combined, 3)
            .build()
            .unwrap();

        let mut node = TextNode::new("Hello duckdb!")
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

    #[test_log::test(tokio::test)]
    async fn test_running_setup_twice() {
        let client = Duckdb::builder()
            .connection(duckdb::Connection::open_in_memory().unwrap())
            .table_name("test".to_string())
            .with_vector(EmbeddedField::Combined, 3)
            .build()
            .unwrap();

        client.setup().await.unwrap();
        client.setup().await.unwrap(); // Should not panic or error
    }

    #[test_log::test(tokio::test)]
    async fn test_persisted() {
        let temp_db_path = temp_dir::TempDir::new().unwrap();
        let temp_db_path = temp_db_path.path().join("test_duckdb.db");

        let client = Duckdb::builder()
            .connection(duckdb::Connection::open(temp_db_path).unwrap())
            .table_name("test".to_string())
            .with_vector(EmbeddedField::Combined, 3)
            .build()
            .unwrap();

        let mut node = TextNode::new("Hello duckdb!")
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
                ))
            })
            .unwrap();

        let retrieved = node_iter.collect::<Result<Vec<_>, _>>().unwrap();
        dbg!(&retrieved);
        //
        assert_eq!(retrieved.len(), 1);
    }
}
