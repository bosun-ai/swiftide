//! This crate provides macros for generating boilerplate code
//! for indexing transformers
use proc_macro::TokenStream;

mod indexing_transformer;
use indexing_transformer::indexing_transformer_impl;
use syn::{parse_macro_input, ItemStruct};

/// Generates boilerplate for an indexing transformer.
#[proc_macro_attribute]
pub fn indexing_transformer(args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemStruct);
    indexing_transformer_impl(args.into(), input).into()
}
