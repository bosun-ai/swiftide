//! Default tools and executor for agents
pub mod arg_preprocessor;
pub mod control;
pub mod local_executor;

/// Add tools from a Model Context Protocol endpoint
#[cfg(feature = "mcp")]
pub mod mcp;
