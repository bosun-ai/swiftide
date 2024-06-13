/// The `integrations` module provides conditional compilation for various external service integrations.
///
/// This module includes sub-modules for different integrations based on the enabled features. Each sub-module
/// corresponds to a specific external service, allowing the Swiftide system to extend its functionality
/// dynamically. Conditional compilation ensures that only the necessary code is included, optimizing performance
/// and reducing the binary size.
///
/// The following integrations are available:
/// - OpenAI
/// - Qdrant
/// - Redis
/// - Tree-sitter

#[cfg(feature = "openai")]
/// The `openai` module provides integration with the OpenAI API.
///
/// This module is included only if the `openai` feature is enabled. It allows the Swiftide system to interact
/// with OpenAI services, enabling functionalities such as natural language processing and other AI-driven features.
pub mod openai;

#[cfg(feature = "qdrant")]
/// The `qdrant` module provides integration with the Qdrant vector search engine.
///
/// This module is included only if the `qdrant` feature is enabled. It allows the Swiftide system to perform
/// efficient vector searches, which is essential for tasks like similarity search and nearest neighbor search.
pub mod qdrant;

#[cfg(feature = "redis")]
/// The `redis` module provides integration with Redis, an in-memory data structure store.
///
/// This module is included only if the `redis` feature is enabled. It allows the Swiftide system to use Redis
/// for caching, real-time analytics, and other data storage needs that benefit from Redis's high performance.
pub mod redis;

#[cfg(feature = "tree-sitter")]
/// The `treesitter` module provides integration with Tree-sitter, a parser generator tool and incremental parsing library.
///
/// This module is included only if the `tree-sitter` feature is enabled. It allows the Swiftide system to use
/// Tree-sitter for parsing and analyzing source code, which is useful for tasks like syntax highlighting,
/// code navigation, and refactoring.
pub mod treesitter;
