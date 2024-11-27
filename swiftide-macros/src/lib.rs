//! This crate provides macros for generating boilerplate code
//! for indexing transformers
use proc_macro::TokenStream;

mod indexing_transformer;
#[cfg(test)]
mod test_utils;
mod tool;
use indexing_transformer::indexing_transformer_impl;
use syn::{parse_macro_input, DeriveInput, ItemFn, ItemStruct};
use tool::{tool_derive_impl, tool_impl};

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
/// pub async fn search_code(context: &dyn AgentContext, code_query: &str) -> Result<ToolOutput,
/// ToolError> {
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

/// Derive tool on a struct. The macro expects a snake case method on the struct that takes the
/// equally named params as `&str` arguments.
///
/// Useful if your structs have internal state and you want to use it in your tool.
///
/// # Example
/// ```ignore
/// #[derive(Clone, Tool)]
/// #[tool(description = "Searches code", param(name = "code_query", description = "The code query"))]
/// pub struct SearchCode {
///   search_command: String
/// }
///
/// impl SearchCode {
///   pub async fn search_code(&self, context: &dyn AgentContext, code_query: &str) -> Result<ToolOutput, ToolError> {
///     context.exec_cmd(&self.search_command.into()).await.map(Into::into)
///   }
/// }
///
/// ```
///
#[proc_macro_derive(Tool, attributes(tool))]
pub fn derive_tool(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match tool_derive_impl(&input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.into_compile_error().into(),
    }
}
