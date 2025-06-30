use qdrant_client::qdrant::ScrollPointsBuilder;
use std::env::var;
use swiftide::indexing::transformers::ChunkText;
use swiftide::traits::NodeCache;
use swiftide::{indexing::Node, integrations};
use swiftide_indexing::Pipeline;
use swiftide_indexing::transformers::Embed;
use swiftide_integrations::duckdb::Duckdb;
use swiftide_integrations::qdrant::Qdrant;
use swiftide_integrations::redb::Redb;
use swiftide_integrations::redis::Redis;
use swiftide_test_utils::{
    mock_chat_completions, mock_embeddings, openai_client, start_qdrant, start_redis,
};
use temp_dir::TempDir;
use wiremock::MockServer;

/// Tests that chunking works correctly with deferred caching using Redis.
///
/// This test verifies that:
/// 1. Nodes are only cached AFTER successful pipeline completion (deferred caching)
/// 2. Chunking creates proper parent-child relationships with parent_id
/// 3. Failed pipelines don't result in caching
/// 4. Successful re-runs use cached data properly
#[test_log::test(tokio::test)]
async fn test_chunking_with_redis_cache() {
    let mock_server = MockServer::start().await;
    mock_chat_completions(&mock_server).await;
    mock_embeddings(&mock_server, 10).await;

    let openai_client = openai_client(&mock_server.uri(), "text-embedding-3-small", "gpt-4o");
    let (_redis, redis_url) = start_redis().await;
    let (_qdrant, qdrant_url) = start_qdrant().await;

    let qdrant_url = var("QDRANT_URL").unwrap_or(qdrant_url);

    let test_nodes = vec![
        Node::new(
            "This is a much longer piece of text that should definitely be split into multiple chunks when processed by the text chunker. It contains enough content to create several meaningful chunks.",
        ),
        Node::new(
            "Another substantial piece of text content that will be divided into smaller, more manageable chunks during the chunking process. This ensures we can test the parent-child relationships properly.",
        ),
    ];

    let original_node_ids: Vec<uuid::Uuid> = test_nodes.iter().map(Node::id).collect();

    let redis_cache =
        Redis::try_from_url(&redis_url, "chunking_test").expect("Failed to create Redis cache");

    for node in &test_nodes {
        let cached = redis_cache.get(node).await;
        assert!(!cached, "Cache should be empty initially");
    }

    let result = Pipeline::from_stream(test_nodes.clone())
        .with_default_llm_client(openai_client.clone())
        .then_chunk(ChunkText::from_max_characters(50))
        .then_in_batch(Embed::new(openai_client.clone()).with_batch_size(10))
        .filter_cached(redis_cache.clone())
        .log_nodes()
        .then_store_with(
            Qdrant::try_from_url(&qdrant_url)
                .unwrap()
                .vector_size(1536)
                .collection_name("chunking-cache-test".to_string())
                .build()
                .unwrap(),
        )
        .run()
        .await;

    result.expect("Chunking with cache pipeline failed");

    let qdrant_client = qdrant_client::Qdrant::from_url(&qdrant_url)
        .build()
        .unwrap();

    let stored_points = qdrant_client
        .scroll(
            ScrollPointsBuilder::new("chunking-cache-test")
                .limit(100)
                .with_payload(true),
        )
        .await
        .unwrap();

    assert!(
        stored_points.result.len() > original_node_ids.len(),
        "Expected more chunks than original nodes. Got {} chunks from {} original nodes",
        stored_points.result.len(),
        original_node_ids.len()
    );

    for point in &stored_points.result {
        if let Some(parent_id_value) = point.payload.get("parent_id") {
            let parent_id_str = parent_id_value
                .as_str()
                .expect("parent_id should be a string");
            let parent_id =
                uuid::Uuid::parse_str(parent_id_str).expect("parent_id should be a valid UUID");

            assert!(
                original_node_ids.contains(&parent_id),
                "Chunk parent_id {parent_id} should be one of the original node IDs",
            );
        }
    }

    for node in &test_nodes {
        let cached = redis_cache.get(node).await;
        assert!(
            cached,
            "Node {} should be cached after successful pipeline completion",
            node.id()
        );
    }

    // Cached nodes should have been stored in Redis
    mock_embeddings(&mock_server, 0).await;

    let second_result = Pipeline::from_stream(test_nodes)
        .with_default_llm_client(openai_client.clone())
        .filter_cached(redis_cache)
        .then_chunk(ChunkText::from_max_characters(50))
        .then_in_batch(Embed::new(openai_client.clone()).with_batch_size(10))
        .then_store_with(
            integrations::qdrant::Qdrant::try_from_url(&qdrant_url)
                .unwrap()
                .vector_size(1536)
                .collection_name("chunking-cache-test-2".to_string())
                .build()
                .unwrap(),
        )
        .run()
        .await;

    second_result.expect("Second run should succeed using cached data");
}

/// Tests that chunking works correctly with deferred caching using Redb.
#[test_log::test(tokio::test)]
async fn test_chunking_with_redb_cache() {
    let mock_server = MockServer::start().await;
    mock_chat_completions(&mock_server).await;
    mock_embeddings(&mock_server, 10).await;

    let openai_client = openai_client(&mock_server.uri(), "text-embedding-3-small", "gpt-4o");
    let (_qdrant, qdrant_url) = start_qdrant().await;

    let qdrant_url = var("QDRANT_URL").unwrap_or(qdrant_url);

    // Create temporary directory for redb database
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test_cache.redb");

    let test_nodes = vec![
        Node::new(
            "This is a much longer piece of text that should definitely be split into multiple chunks when processed by the text chunker. It contains enough content to create several meaningful chunks.",
        ),
        Node::new(
            "Another substantial piece of text content that will be divided into smaller, more manageable chunks during the chunking process. This ensures we can test the parent-child relationships properly.",
        ),
    ];

    let original_node_ids: Vec<uuid::Uuid> = test_nodes.iter().map(Node::id).collect();

    let redb_cache = Redb::builder()
        .database_path(db_path)
        .table_name("chunking_test")
        .cache_key_prefix("test")
        .build()
        .expect("Failed to create Redb cache");

    // Verify cache is initially empty
    for node in &test_nodes {
        let cached = redb_cache.get(node).await;
        assert!(!cached, "Cache should be empty initially");
    }

    let result = Pipeline::from_stream(test_nodes.clone())
        .with_default_llm_client(openai_client.clone())
        .then_chunk(ChunkText::from_max_characters(50))
        .then_in_batch(Embed::new(openai_client.clone()).with_batch_size(10))
        .filter_cached(redb_cache.clone())
        .log_nodes()
        .then_store_with(
            Qdrant::try_from_url(&qdrant_url)
                .unwrap()
                .vector_size(1536)
                .collection_name("redb-chunking-cache-test".to_string())
                .build()
                .unwrap(),
        )
        .run()
        .await;

    result.expect("Chunking with Redb cache pipeline failed");

    let qdrant_client = qdrant_client::Qdrant::from_url(&qdrant_url)
        .build()
        .unwrap();

    let stored_points = qdrant_client
        .scroll(
            ScrollPointsBuilder::new("redb-chunking-cache-test")
                .limit(100)
                .with_payload(true),
        )
        .await
        .unwrap();

    assert!(
        stored_points.result.len() > original_node_ids.len(),
        "Expected more chunks than original nodes. Got {} chunks from {} original nodes",
        stored_points.result.len(),
        original_node_ids.len()
    );

    // Verify parent-child relationships
    for point in &stored_points.result {
        if let Some(parent_id_value) = point.payload.get("parent_id") {
            let parent_id_str = parent_id_value
                .as_str()
                .expect("parent_id should be a string");
            let parent_id =
                uuid::Uuid::parse_str(parent_id_str).expect("parent_id should be a valid UUID");

            assert!(
                original_node_ids.contains(&parent_id),
                "Chunk parent_id {parent_id} should be one of the original node IDs",
            );
        }
    }

    // Verify nodes are cached after successful completion
    for node in &test_nodes {
        let cached = redb_cache.get(node).await;
        assert!(
            cached,
            "Node {} should be cached after successful pipeline completion",
            node.id()
        );
    }

    // Test second run uses cached data
    mock_embeddings(&mock_server, 0).await;

    let second_result = Pipeline::from_stream(test_nodes)
        .with_default_llm_client(openai_client.clone())
        .filter_cached(redb_cache)
        .then_chunk(ChunkText::from_max_characters(50))
        .then_in_batch(Embed::new(openai_client.clone()).with_batch_size(10))
        .then_store_with(
            integrations::qdrant::Qdrant::try_from_url(&qdrant_url)
                .unwrap()
                .vector_size(1536)
                .collection_name("redb-chunking-cache-test-2".to_string())
                .build()
                .unwrap(),
        )
        .run()
        .await;

    second_result.expect("Second run should succeed using cached data");
}

/// Tests that chunking works correctly with deferred caching using DuckDB.
#[test_log::test(tokio::test)]
async fn test_chunking_with_duckdb_cache() {
    let mock_server = MockServer::start().await;
    mock_chat_completions(&mock_server).await;
    mock_embeddings(&mock_server, 10).await;

    let openai_client = openai_client(&mock_server.uri(), "text-embedding-3-small", "gpt-4o");
    let (_qdrant, qdrant_url) = start_qdrant().await;

    let qdrant_url = var("QDRANT_URL").unwrap_or(qdrant_url);

    let test_nodes = vec![
        Node::new(
            "This is a much longer piece of text that should definitely be split into multiple chunks when processed by the text chunker. It contains enough content to create several meaningful chunks.",
        ),
        Node::new(
            "Another substantial piece of text content that will be divided into smaller, more manageable chunks during the chunking process. This ensures we can test the parent-child relationships properly.",
        ),
    ];

    let original_node_ids: Vec<uuid::Uuid> = test_nodes.iter().map(Node::id).collect();

    let duckdb_cache = Duckdb::builder()
        .connection(duckdb::Connection::open_in_memory().unwrap())
        .build()
        .unwrap();

    // Initialize cache table
    duckdb_cache
        .lazy_create_cache()
        .await
        .expect("Failed to create cache table");

    // Verify cache is initially empty
    for node in &test_nodes {
        let cached = duckdb_cache.get(node).await;
        assert!(!cached, "Cache should be empty initially");
    }

    let result = Pipeline::from_stream(test_nodes.clone())
        .with_default_llm_client(openai_client.clone())
        .then_chunk(ChunkText::from_max_characters(50))
        .then_in_batch(Embed::new(openai_client.clone()).with_batch_size(10))
        .filter_cached(duckdb_cache.clone())
        .log_nodes()
        .then_store_with(
            Qdrant::try_from_url(&qdrant_url)
                .unwrap()
                .vector_size(1536)
                .collection_name("duckdb-chunking-cache-test".to_string())
                .build()
                .unwrap(),
        )
        .run()
        .await;

    result.expect("Chunking with DuckDB cache pipeline failed");

    let qdrant_client = qdrant_client::Qdrant::from_url(&qdrant_url)
        .build()
        .unwrap();

    let stored_points = qdrant_client
        .scroll(
            ScrollPointsBuilder::new("duckdb-chunking-cache-test")
                .limit(100)
                .with_payload(true),
        )
        .await
        .unwrap();

    assert!(
        stored_points.result.len() > original_node_ids.len(),
        "Expected more chunks than original nodes. Got {} chunks from {} original nodes",
        stored_points.result.len(),
        original_node_ids.len()
    );

    // Verify parent-child relationships
    for point in &stored_points.result {
        if let Some(parent_id_value) = point.payload.get("parent_id") {
            let parent_id_str = parent_id_value
                .as_str()
                .expect("parent_id should be a string");
            let parent_id =
                uuid::Uuid::parse_str(parent_id_str).expect("parent_id should be a valid UUID");

            assert!(
                original_node_ids.contains(&parent_id),
                "Chunk parent_id {parent_id} should be one of the original node IDs",
            );
        }
    }

    // Verify nodes are cached after successful completion
    for node in &test_nodes {
        let cached = duckdb_cache.get(node).await;
        assert!(
            cached,
            "Node {} should be cached after successful pipeline completion",
            node.id()
        );
    }

    // Test second run uses cached data
    mock_embeddings(&mock_server, 0).await;

    let second_result = Pipeline::from_stream(test_nodes)
        .with_default_llm_client(openai_client.clone())
        .filter_cached(duckdb_cache)
        .then_chunk(ChunkText::from_max_characters(50))
        .then_in_batch(Embed::new(openai_client.clone()).with_batch_size(10))
        .then_store_with(
            integrations::qdrant::Qdrant::try_from_url(&qdrant_url)
                .unwrap()
                .vector_size(1536)
                .collection_name("duckdb-chunking-cache-test-2".to_string())
                .build()
                .unwrap(),
        )
        .run()
        .await;

    second_result.expect("Second run should succeed using cached data");
}

/// Tests that failed pipeline doesn't result in caching with Redis
#[test_log::test(tokio::test)]
async fn test_failed_pipeline_no_redis_cache() {
    let (_redis, redis_url) = start_redis().await;

    let test_nodes = vec![Node::new(
        "Test node that should not be cached due to pipeline failure",
    )];

    let redis_cache =
        Redis::try_from_url(&redis_url, "fail_test").expect("Failed to create Redis cache");

    let node = &test_nodes[0];
    assert!(
        !redis_cache.get(node).await,
        "Cache should be empty initially"
    );

    let result = Pipeline::from_stream(test_nodes.clone())
        .filter_cached(redis_cache.clone())
        .then_chunk(ChunkText::from_max_characters(50))
        .run()
        .await;

    assert!(result.is_err(), "Pipeline should fail without storage");

    assert!(
        !redis_cache.get(node).await,
        "Node should NOT be cached after pipeline failure"
    );
}

/// Tests that failed pipeline doesn't result in caching with Redb
#[test_log::test(tokio::test)]
async fn test_failed_pipeline_no_redb_cache() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test_fail_cache.redb");

    let test_nodes = vec![Node::new(
        "Test node that should not be cached due to pipeline failure",
    )];

    let redb_cache = Redb::builder()
        .database_path(db_path)
        .table_name("fail_test")
        .cache_key_prefix("test")
        .build()
        .expect("Failed to create Redb cache");

    let node = &test_nodes[0];
    assert!(
        !redb_cache.get(node).await,
        "Cache should be empty initially"
    );

    let result = Pipeline::from_stream(test_nodes.clone())
        .filter_cached(redb_cache.clone())
        .then_chunk(ChunkText::from_max_characters(50))
        .run()
        .await;

    assert!(result.is_err(), "Pipeline should fail without storage");

    assert!(
        !redb_cache.get(node).await,
        "Node should NOT be cached after pipeline failure"
    );
}

/// Tests that failed pipeline doesn't result in caching with DuckDB
#[test_log::test(tokio::test)]
async fn test_failed_pipeline_no_duckdb_cache() {
    let test_nodes = vec![Node::new(
        "Test node that should not be cached due to pipeline failure",
    )];

    let duckdb_cache = Duckdb::builder()
        .connection(duckdb::Connection::open_in_memory().unwrap())
        .build()
        .unwrap();

    // Initialize cache table
    duckdb_cache
        .lazy_create_cache()
        .await
        .expect("Failed to create cache table");

    let node = &test_nodes[0];
    assert!(
        !duckdb_cache.get(node).await,
        "Cache should be empty initially"
    );

    let result = Pipeline::from_stream(test_nodes.clone())
        .filter_cached(duckdb_cache.clone())
        .then_chunk(ChunkText::from_max_characters(50))
        .run()
        .await;

    assert!(result.is_err(), "Pipeline should fail without storage");

    assert!(
        !duckdb_cache.get(node).await,
        "Node should NOT be cached after pipeline failure"
    );
}
