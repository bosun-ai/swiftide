//! This module provides the integration with Redis for caching nodes in the Swiftide system.
//!
//! The primary component of this module is the `RedisNodeCache`, which is re-exported for use
//! in other parts of the system. The `RedisNodeCache` struct is responsible for managing and
//! caching nodes during the ingestion process, leveraging Redis for efficient storage and retrieval.
//!
//! # Overview
//!
//! The `RedisNodeCache` struct provides methods for:
//! - Connecting to a Redis database
//! - Checking if a node is cached
//! - Setting a node in the cache
//! - Resetting the cache (primarily for testing purposes)
//!
//! This integration is essential for ensuring efficient node management and caching in the Swiftide system.

mod node_cache;

pub use node_cache::RedisNodeCache;
