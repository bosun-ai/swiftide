//! This module enables the implementation of chat completion on LLM providers
//!
//! The main trait to implement is `ChatCompletion`, which takes a `ChatCompletionRequest` and
//! returns a `ChatCompletionResponse`.
//!
//! A chat completion request is comprised of a list of `ChatMessage` to complete, with
//! optionally tool specifications. The builder provides fluent helpers like `message(...)`
//! and `tools(...)` so you can accumulate messages and register tool instances directly
//! while still exposing `tool_specs` for compatibility.
mod chat_completion_request;
mod chat_completion_response;
mod chat_message;
pub mod errors;
mod tools;

// Re-exported in the root per convention
pub mod traits;

pub use chat_completion_request::*;
pub use chat_completion_response::*;
pub use chat_message::*;
pub use tools::*;
pub use traits::*;
