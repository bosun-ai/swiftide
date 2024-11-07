//! This crate provides macros for generating boilerplate code
//! for indexing transformers
use proc_macro::TokenStream;

mod indexing_transformer;
#[cfg(test)]
mod test_utils;
mod tool;
use indexing_transformer::indexing_transformer_impl;
use syn::{parse_macro_input, ItemFn, ItemStruct};
use tool::tool_impl;

/// Generates boilerplate for an indexing transformer.
#[proc_macro_attribute]
pub fn indexing_transformer(args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemStruct);
    indexing_transformer_impl(args.into(), input).into()
}

#[proc_macro_attribute]
/// Creates a tool from an async function.
///
/// # Example
/// ```ignore
/// #[tool(description = "Searches code", param(name = "code_query", description = "The code query"))]
/// pub async fn search_code(context: &dyn AgentContext, code_query: &str) -> Result<ToolOutput> {
///    Ok("hello".into())
/// }
///
/// // The tool can then be used with agents:
/// Agent::builder().tools([search_code()])
///
/// // Or
///
/// Agent::builder().tools([SearchCode::default()])
///
/// ```
///
pub fn tool(args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemFn);
    tool_impl(&args.into(), &input).into()
}
