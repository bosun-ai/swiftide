use darling::{ast::NestedMeta, Error, FromMeta};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{token::Pub, Field, Fields, Ident, ItemFn, ItemStruct, Token};

mod args;
mod wrapped;

#[derive(FromMeta, Default)]
#[darling(default)]
struct ToolArgs {
    name: String,
    description: String,

    #[darling(multiple)]
    param: Vec<ParamOptions>,
}

#[derive(FromMeta, Debug, Default)]
#[darling(default)]
struct ParamOptions {
    name: String,
    description: String,
    // TODO: I.e. openai also supports enums instead of strings as arg type
}

#[allow(clippy::too_many_lines)]
pub(crate) fn tool_impl(args: TokenStream, input: ItemFn) -> TokenStream {
    let args = match parse_args(args) {
        Ok(args) => args,
        Err(e) => return e.write_errors(),
    };

    let tool_args = args::build_tool_args(&input).unwrap_or_else(syn::Error::into_compile_error);
    let wrapped_fn = wrapped::wrap_tool_fn(&input).unwrap_or_else(syn::Error::into_compile_error);

    // Building the args struct
    quote! {
        #tool_args

        #wrapped_fn
        // The args
        // new wrapper function that takes parsed args and calls old function
        // old function
        // Tool impl
    }
}

fn parse_args(args: TokenStream) -> Result<ToolArgs, Error> {
    let attr_args = NestedMeta::parse_meta_list(args)?;

    ToolArgs::from_list(&attr_args)
}

#[cfg(test)]
mod tests {
    use crate::assert_ts_eq;

    use super::*;
    use quote::quote;
    use syn::{parse_quote, ItemFn};

    // #[test]
    // fn test_simple_tool() {
    //     let args = quote! {
    //         name = "Hello world",
    //         description = "Hello world tool",
    //         param(
    //             name = "name",
    //             description = "Your name"
    //         )
    //     };
    //     let input: ItemFn = parse_quote! {
    //         pub async fn search_code(context: &impl AgentContext, code_query: &str) -> Result<ToolOutput> {
    //             return Ok("hello".into())
    //         }
    //     };
    //
    //     let output = tool_impl(args, input);
    //
    //     let expected = quote! {
    //         struct HelloWorld {
    //             pub code_query: &str,
    //         }
    //     };
    //
    //     assert_ts_eq!(&output, &expected);
    // }
}
