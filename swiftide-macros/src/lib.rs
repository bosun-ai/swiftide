//! This crate provides macros for generating boilerplate code
//! for indexing transformers
use darling::{ast::NestedMeta, Error, FromMeta};
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, MetaList};

mod indexing_transformer;
use indexing_transformer::indexing_transformer_impl;

/// Generates boilerplate for an indexing transformer.
#[proc_macro_attribute]
pub fn indexing_transformer(args: TokenStream, item: TokenStream) -> TokenStream {
    indexing_transformer_impl(args, item)
}
