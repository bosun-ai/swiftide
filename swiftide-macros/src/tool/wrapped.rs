use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    parse::Result, FnArg, Ident, ItemFn,
    Pat, PatType,
};

use super::args::args_struct_name;

pub(crate) fn struct_name(input: &ItemFn) -> Ident {
    let struct_name_str = input
        .sig
        .ident
        .to_string()
        .split('_') // Split by underscores
        .map(|s| {
            let mut chars = s.chars();
            chars
                .next()
                .map(|c| c.to_ascii_uppercase())
                .into_iter()
                .collect::<String>()
                + chars.as_str()
        })
        .collect::<String>();
    Ident::new(&struct_name_str, input.sig.ident.span())
}

pub(crate) fn wrap_tool_fn(input: &ItemFn) -> Result<TokenStream> {
    let fn_name = &input.sig.ident;
    let fn_args = &input.sig.inputs;
    let fn_body = &input.block;
    let fn_output = &input.sig.output;
    let underscored_fn_name = Ident::new(&format!("_{fn_name}"), fn_name.span());

    let struct_name = struct_name(input);
    let args_struct_name = args_struct_name(input);

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
        #[derive(Clone)]
        struct #struct_name {}

        pub fn #fn_name() -> #struct_name {
            #struct_name {}
        }

        impl #struct_name {
            pub async fn #fn_name(&self, #(#fn_args),*) #fn_output #fn_body
        }

    })
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
            pub async fn search_code(context: &dyn hidden::AgentContext, code_query: &str) -> hidden::Result<hidden::ToolOutput> {
                return Ok("hello".into())
            }
        };

        let output = wrap_tool_fn(&input).unwrap();

        let expected = quote! {
            #[derive(Clone)]
            struct SearchCode {}

            pub fn search_code() -> SearchCode {
                SearchCode {}
            }

            impl SearchCode {
                pub async fn search_code(&self, context: &dyn hidden::AgentContext, code_query: &str) -> hidden::Result<hidden::ToolOutput> {
                    return Ok("hello".into())
                }

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
