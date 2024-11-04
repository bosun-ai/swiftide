//! This crate provides macros for generating boilerplate code
//! for indexing transformers
use proc_macro::TokenStream;

mod indexing_transformer;
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
pub fn took(args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemFn);
    tool_impl(args.into(), input).into()
}
