use proc_macro2::TokenStream;
use quote::{quote, ToTokens as _};
use syn::{parse::Result, Error, FnArg, Ident, ItemFn, Lifetime, PatType, Type, TypeReference};

pub(crate) fn args_struct_name(input: &ItemFn) -> Ident {
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
        .collect::<String>()
        + "Args";
    Ident::new(&struct_name_str, input.sig.ident.span())
}

pub(crate) fn build_tool_args(input: &ItemFn) -> Result<TokenStream> {
    validate_first_argument_is_agent_context(input)?;

    let args = &input.sig.inputs;
    let mut struct_fields = Vec::new();

    for arg in args.iter().skip(1) {
        if let syn::FnArg::Typed(PatType { pat, ty, .. }) = arg {
            if let syn::Pat::Ident(ident) = &**pat {
                struct_fields.push(quote! { pub #ident: String });
            }
        }
    }

    if struct_fields.is_empty() {
        return Ok(quote! {});
    }

    let struct_name = args_struct_name(input);

    Ok(quote! {
        #[derive(::swiftide::reexports::serde::Serialize, ::swiftide::reexports::serde::Deserialize)]
        struct #struct_name {
            #(#struct_fields),*
        }
    })
}

fn validate_first_argument_is_agent_context(input_fn: &ItemFn) -> Result<()> {
    let expected_first_arg = quote! { &dyn AgentContext };
    let error_msg = "The first argument must be `&dyn AgentContext`";

    if let Some(FnArg::Typed(first_arg)) = input_fn.sig.inputs.first() {
        if first_arg.ty.to_token_stream().to_string() != expected_first_arg.to_string() {
            return Err(Error::new_spanned(&first_arg.ty, error_msg));
        }
    } else {
        return Err(Error::new_spanned(&input_fn.sig, error_msg));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::assert_ts_eq;

    use super::*;
    use quote::quote;
    use syn::{parse_quote, ItemFn};

    #[test]
    fn test_agent_context_as_first_arg_required() {
        let input: ItemFn = parse_quote! {
            pub async fn search_code(code_query: &str) -> Result<ToolOutput> {
                return Ok("hello".into())
            }
        };

        let output = build_tool_args(&input).unwrap_err();

        assert_eq!(
            output.to_string(),
            "The first argument must be `&dyn AgentContext`"
        );
    }

    #[test]
    fn test_agent_multiple_args() {
        let input: ItemFn = parse_quote! {
            pub async fn search_code(context: &dyn AgentContext, code_query: &str, other: &str) -> Result<ToolOutput> {
                return Ok("hello".into())
            }
        };

        let output = build_tool_args(&input).unwrap();

        let expected = quote! {
            #[derive(::swiftide::reexports::serde::Serialize, ::swiftide::reexports::serde::Deserialize)]
            struct SearchCodeArgs {
                pub code_query: String,
                pub other: String
            }
        };

        assert_ts_eq!(&output, &expected);
    }

    #[test]
    fn test_simple_tool_with_lifetime() {
        let input: ItemFn = parse_quote! {
            pub async fn search_code(context: &dyn AgentContext, code_query: &str) -> Result<ToolOutput> {
                return Ok("hello".into())
            }
        };

        let output = build_tool_args(&input).unwrap();

        let expected = quote! {
            #[derive(::swiftide::reexports::serde::Serialize, ::swiftide::reexports::serde::Deserialize)]
            struct SearchCodeArgs {
                pub code_query: String,
            }
        };

        assert_ts_eq!(&output, &expected);
    }

    #[test]
    fn test_simple_tool_without_lifetime() {
        let input: ItemFn = parse_quote! {
            pub async fn search_code(context: &dyn AgentContext, code_query: String) -> Result<ToolOutput> {
                return Ok("hello".into())
            }
        };

        let output = build_tool_args(&input).unwrap();

        let expected = quote! {
            #[derive(::swiftide::reexports::serde::Serialize, ::swiftide::reexports::serde::Deserialize)]
            struct SearchCodeArgs {
                pub code_query: String,
            }
        };

        assert_ts_eq!(&output, &expected);
    }

    #[test]
    fn test_no_arguments() {
        let input: ItemFn = parse_quote! {
            pub async fn search_code(context: &dyn AgentContext) -> Result<ToolOutput> {
                return Ok("hello".into())
            }
        };
        let output = build_tool_args(&input).unwrap();
        let expected = quote! {};
        assert_ts_eq!(&output, &expected);
    }

    // TODO: Handle no arguments
    // TODO: Should it only allow &str as arg types?
}
