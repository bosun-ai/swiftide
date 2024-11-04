use proc_macro2::TokenStream;
use quote::{quote, ToTokens as _};
use syn::{
    parse::Result, token::Pub, Error, Field, Fields, FnArg, Ident, ItemFn, ItemStruct, Lifetime,
    Pat, PatType, Token, Type, TypeReference,
};

use super::args::args_struct_name;

pub(crate) fn wrap_tool_fn(input: &ItemFn) -> Result<TokenStream> {
    let fn_name = &input.sig.ident;
    let fn_args = &input.sig.inputs;
    let fn_body = &input.block;
    let underscored_fn_name = Ident::new(&format!("_{}", fn_name), fn_name.span());

    let struct_name = args_struct_name(input);

    let arg_names = fn_args.iter().skip(1).filter_map(|arg| {
        if let FnArg::Typed(PatType { pat, .. }) = arg {
            if let Pat::Ident(ident) = &**pat {
                Some(quote! { args.#ident })
            } else {
                None
            }
        } else {
            None
        }
    });

    let fn_args = fn_args.iter();

    Ok(quote! {
        pub async fn #fn_name(context: &impl AgentContext, args: #struct_name<'_>) -> Result<ToolOutput> {
            #underscored_fn_name(context, #(#arg_names),*).await
        }

        async fn #underscored_fn_name(#(#fn_args),*) -> Result<ToolOutput> #fn_body
    })
}

pub(crate) fn wrapped_fn_sig(input: &ItemFn) -> TokenStream {
    let struct_name = args_struct_name(input);

    quote! {
        Fn(&impl AgentContext, #struct_name<'_>) -> Result<ToolOutput>
    }
}

#[cfg(test)]
mod tests {
    use crate::assert_ts_eq;

    use super::*;
    use quote::quote;
    use syn::{parse_quote, ItemFn};

    #[test]
    fn test_wrap_tool_fn() {
        let input: ItemFn = parse_quote! {
            pub async fn search_code(context: &impl AgentContext, code_query: &str) -> Result<ToolOutput> {
                return Ok("hello".into())
            }
        };

        let output = wrap_tool_fn(&input).unwrap();

        let expected = quote! {
            pub async fn search_code(context: &impl AgentContext, args: SearchCodeArgs<'_>) -> Result<ToolOutput> {
                _search_code(context, args.code_query).await
            }

            async fn _search_code(context: &impl AgentContext, code_query: &str) -> Result<ToolOutput> {
                return Ok("hello".into())
            }

        };

        assert_ts_eq!(&output, &expected);
    }
}
// test cases
// support async only
// allow no args
// work with and without lifetime on args
// Asserts always returns a tool output result
